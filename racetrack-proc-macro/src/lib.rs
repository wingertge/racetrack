extern crate proc_macro;
#[macro_use]
extern crate syn;

use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned, ToTokens};
use syn::{
    punctuated::Punctuated, spanned::Spanned, AttributeArgs, Expr, ExprAssign, ExprClosure, FnArg,
    ImplItem, ImplItemMethod, Index, Item, ItemFn, ItemImpl, Lit, Local, Meta, MetaNameValue,
    NestedMeta, Pat, PatIdent, PatType, Stmt
};

#[inline]
fn unsupported() -> TokenStream {
    quote! {
        compile_error!("Unsupported attribute target. 'track_with' only supports functions, impl blocks and closures.");
    }
}

/// Track the target with the tracker specified in the arguments.
/// Requires one argument containing the path to the tracker.
///
/// # Arguments
///
/// * `tracked_path` - The path to the tracker. This must be the first unnamed argument. Required.
/// * `exclude` - A comma separated list of methods to exclude. This only does something on impl blocks.
/// * `include_receiver` - Include the receiver (self). If false, the tracker must be available in the scope of the relevant method.
///     If no receiver was found and this is true, the method will be skipped. Defaults to true.
/// * `namespace` - Override the namespace of the tracked item. Tracked key will be namespace::function_name.
///     Defaults to the struct name for impl blocks, None for functions and closures.
///
/// # Example
///
/// ```
/// # use std::sync::Arc;
/// use racetrack::{Tracker, track_with};
///
/// struct TrackedStruct(Arc<Tracker>);
///
/// #[track_with(0, namespace = "Tracked")]
/// impl TrackedStruct {
///     fn tracked_fn(&self, arg: String) {}
/// }
/// ```
#[proc_macro_attribute]
pub fn track_with(
    args: proc_macro::TokenStream,
    item_tokens: proc_macro::TokenStream
) -> proc_macro::TokenStream {
    let args = syn::parse_macro_input!(args as AttributeArgs);
    let args = parse_args(args);
    //println!("{:?}", args);

    let item = syn::parse::<Item>(item_tokens.clone());

    let tokens = match item {
        Ok(Item::Fn(fun)) => track_function(&args, fun),
        Ok(Item::Impl(item)) => track_impl(&args, item),
        Ok(Item::Struct(_)) => quote! {
            compile_error!("Structs aren't a supported attribute target. To track methods, put this attribute on an impl block.")
        },
        Err(_) => {
            if let Ok(stmt) = syn::parse::<Stmt>(item_tokens.clone()) {
                let tokens = match stmt {
                    Stmt::Local(Local {
                        pat, init, attrs, ..
                    }) => {
                        if let Some(Expr::Closure(closure)) = init.map(|expr| *expr.1) {
                            let name = quote!(#pat).to_string();
                            let closure = track_closure(&args, closure, name);
                            quote! {
                                #(#attrs)*
                                let #pat = #closure;
                            }
                        } else {
                            unsupported()
                        }
                    }
                    Stmt::Expr(Expr::Assign(ExprAssign { left, right, .. })) => {
                        if let Expr::Closure(closure) = *right {
                            let name = quote!(#left).to_string();
                            let closure = track_closure(&args, closure, name);
                            quote!(#left = #closure)
                        } else {
                            unsupported()
                        }
                    }
                    _ => unsupported()
                };
                tokens.into()
            } else {
                unsupported()
            }
        }
        _ => unsupported()
    };

    tokens.into()
}

/// Arguments that can be passed to the proc macro
#[derive(Debug)]
struct Arguments {
    /// The path to the tracker. This must be the first unnamed argument.
    tracker_path: TokenStream,
    /// A comma separated list of methods to exclude. This only does something on impl blocks.
    exclude: Vec<String>,
    /// Include the receiver (self). If false, the tracker must be available in the scope of the relevant method.
    /// If no receiver was found and this is true, the method will be skipped. Defaults to true.
    include_receiver: bool,
    /// Override the namespace of the tracked item. Tracked key will be namespace::function_name.
    /// Defaults to the struct name for impl blocks, None for functions and closures.
    namespace: Option<String>
}

fn parse_args(mut args: AttributeArgs) -> Arguments {
    args.reverse();
    let tracker_path = {
        if args.len() == 0 {
            quote_spanned! {
                Span::call_site() =>
                compile_error!("Invalid number of arguments. Expected one argument with the path of the tracker.");
            }
        } else {
            //println!("{:#?}", args);
            let arg = args.pop().unwrap();
            if let NestedMeta::Meta(Meta::Path(path)) = arg {
                quote!(#path)
            } else if let NestedMeta::Lit(Lit::Int(int)) = arg {
                // Tuple struct ident
                let value = int.base10_parse::<usize>().unwrap();
                let index: Index = value.into();
                quote!(#index)
            } else {
                quote_spanned! {
                    arg.span() =>
                    compile_error!("Invalid argument. Should be path of tracker.");
                }
            }
        }
    };
    let mut arguments = Arguments {
        tracker_path,
        exclude: Vec::new(),
        include_receiver: true,
        namespace: None
    };
    while let Some(next) = args.pop() {
        if let NestedMeta::Meta(Meta::NameValue(MetaNameValue { path, lit, .. })) = next {
            if let Some(key) = path.segments.first().map(|path| path.ident.to_string()) {
                match key.as_str() {
                    "exclude" => {
                        if let Lit::Str(str) = lit {
                            let token = str.value();
                            let value: Vec<_> =
                                token.split(",").map(|s| s.trim().to_string()).collect();
                            arguments.exclude = value;
                        } else {
                            panic!("Invalid value for exclude config. Should be comma separated string.");
                        }
                    }
                    "include_receiver" => {
                        if let Lit::Bool(bool) = lit {
                            arguments.include_receiver = bool.value;
                        } else {
                            panic!("Invalid value for include_receiver config. Should be boolean.");
                        }
                    }
                    "namespace" => {
                        if let Lit::Str(str) = lit {
                            arguments.namespace = Some(str.value());
                        } else {
                            panic!("Invalid value for namespace config. Should be a string.");
                        }
                    }
                    _ => {
                        panic!("Unexpected config entry in track_with attribute.");
                    }
                }
            } else {
                panic!("Invalid config entry in track_with attribute.");
            }
        } else {
            panic!("Unexpected argument in track_with attribute.");
        }
    }
    //println!("{:?}", arguments);
    arguments
}

fn track_impl(args: &Arguments, item: ItemImpl) -> TokenStream {
    //println!("{:#?}", item);
    let ItemImpl {
        attrs,
        defaultness,
        unsafety,
        generics,
        trait_,
        self_ty,
        items,
        ..
    } = item;
    let namespace = args
        .namespace
        .as_ref()
        .map(|s| s.clone())
        .unwrap_or_else(|| quote!(#self_ty).to_string());
    let trait_ = trait_.map(|(bang, trait_, for_)| quote!(#bang#trait_ #for_));

    let items = items.iter().map(|item| {
        if let ImplItem::Method(method) = item {
            track_method(&args, method, &namespace)
        } else {
            quote!(#item)
        }
    });

    let tokens = quote! {
        #(#attrs)*
        #defaultness #unsafety impl #generics #trait_ #self_ty {
            #(#items)*
        }
    };

    //println!("{}", tokens);
    tokens
}

fn track_method(args: &Arguments, method: &ImplItemMethod, namespace: &str) -> TokenStream {
    let name = method.sig.ident.to_string();
    if args.exclude.contains(&name) {
        return quote!(#method);
    }
    let name = format!("{}::{}", namespace, name);

    let ImplItemMethod {
        attrs,
        vis,
        defaultness,
        sig,
        block
    } = method;

    let receiver = sig.inputs.iter().find_map(|arg| {
        if let FnArg::Receiver(recv) = arg {
            Some(recv)
        } else {
            None
        }
    });

    if args.include_receiver && receiver.is_none() {
        // Skip static methods since the tracker path won't be valid
        return quote!(#method);
    }

    let inputs_cloned = cloned_inputs(&sig.inputs);
    let result_cloned = quote_spanned! {
        sig.output.span() =>
        returned.to_owned()
    };
    let statements = &block.stmts;
    let tracker_path = &args.tracker_path;
    let tracker_path = if args.include_receiver {
        quote!(self.#tracker_path)
    } else {
        tracker_path.clone()
    };

    let body = quote_spanned! {
        block.span() =>
        let args = (#(#inputs_cloned),*);
        let returned = {
            #(#statements)*
        };
        #tracker_path.log_call(#name, ::racetrack::CallInfo {
            arguments: Some(Box::new(args)),
            returned: Some(Box::new(#result_cloned))
        });
        returned
    };

    let attrs = spanned_vec(attrs);
    let vis = spanned(vis);
    let defaultness = spanned_opt(defaultness.as_ref());
    let sig = spanned(sig);

    let tokens = quote! {
        #(#attrs)*
        #vis #defaultness #sig {
            #body
        }
    };

    tokens
}

fn track_function(args: &Arguments, fun: ItemFn) -> TokenStream {
    //println!("{:#?}", fun);
    let attrs = fun.attrs;
    let visibility = fun.vis;
    let signature = fun.sig;
    let name = if let Some(ref namespace) = args.namespace {
        format!("{}::{}", namespace, signature.ident.to_string())
    } else {
        signature.ident.to_string()
    };
    let arg_idents = cloned_inputs(&signature.inputs);
    let returned_clone = quote_spanned! {
        signature.output.span() =>
        returned.to_owned()
    };
    let block = &fun.block;
    let statements = &fun.block.stmts;
    let tracker_path = &args.tracker_path;
    let body = quote_spanned! {
        block.span() =>
            let args = (#(#arg_idents),*);
            let returned = {
                #(#statements)*
            };
            #tracker_path.log_call(#name, ::racetrack::CallInfo {
                arguments: Some(Box::new(args)),
                returned: Some(Box::new(#returned_clone))
            });
            returned
    };

    let tokens = quote! {
        #(#attrs)*
        #visibility #signature {
            #body
        }
    };

    //println!("{}", tokens);
    tokens
}

fn track_closure(args: &Arguments, closure: ExprClosure, name: String) -> TokenStream {
    let ExprClosure {
        attrs,
        asyncness,
        movability,
        capture,
        inputs,
        output,
        body,
        ..
    } = closure;
    let tracker_path = &args.tracker_path;
    let attrs = spanned_vec(&attrs);
    let asyncness = spanned_opt(asyncness);
    let movability = spanned_opt(movability);
    let capture = spanned_opt(capture);
    let cloned_inputs = cloned_inputs_pat(&inputs);
    let cloned_return = quote_spanned! {
        output.span() =>
        returned.to_owned()
    };
    let inputs: Vec<_> = inputs.iter().map(|input| {
        quote_spanned! {
            input.span() =>
            #input
        }
    }).collect();
    let arguments = &inputs;
    let body_outer = quote_spanned! {
        body.span() =>
        let args = (#(#cloned_inputs),*);
        let returned = inner(#(#arguments)*);
        tracker.log_call(#name, ::racetrack::CallInfo {
            arguments: Some(Box::new(args)),
            returned: Some(Box::new(#cloned_return))
        });
        returned
    };

    let tokens = quote! {
        {
            let inner = #(#attrs)*
            #asyncness #movability #capture |#(#arguments)*| #output {
                #body
            };
            let tracker = #tracker_path.clone();
            #asyncness #movability move |#(#arguments)*| #output {
                #body_outer
            }
        }
    };
    tokens
}

fn spanned(item: impl ToTokens + Spanned) -> TokenStream {
    quote_spanned! {
        item.span() =>
        #item
    }
}

fn spanned_vec<T: ToTokens + Spanned>(item: &Vec<T>) -> Vec<TokenStream> {
    item.iter()
        .map(|item| {
            quote_spanned! {
                item.span() =>
                #item
            }
        })
        .collect()
}

fn spanned_opt<T: ToTokens + Spanned>(item: Option<T>) -> TokenStream {
    item.map(|item| {
        quote_spanned! {
            item.span() =>
            #item
        }
    })
    .unwrap_or_else(|| quote!())
}

fn cloned_inputs<'a>(inputs: &Punctuated<FnArg, Token![,]>) -> Vec<TokenStream> {
    inputs
        .iter()
        .filter_map(|arg| {
            if let FnArg::Typed(PatType { ref pat, .. }) = arg {
                Some(pat)
            } else {
                None
            }
        })
        .filter_map(|arg| {
            if let &Pat::Ident(PatIdent { ref ident, .. }) = &**arg {
                Some(ident)
            } else {
                None
            }
        })
        .map(|ident| {
            quote_spanned! {
                ident.span() =>
                #ident.to_owned()
            }
        })
        .collect()
}

fn cloned_inputs_pat<'a>(inputs: &Punctuated<Pat, Token![,]>) -> Vec<TokenStream> {
    //println!("{:?}", inputs);
    inputs
        .iter()
        .filter_map(|arg| {
            if let Pat::Ident(PatIdent { ref ident, .. }) = arg {
                Some(ident)
            } else if let Pat::Type(PatType { pat, .. }) = arg {
                if let Pat::Ident(PatIdent { ident, .. }) = &**pat {
                    Some(ident)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .map(|ident| {
            quote_spanned! {
                ident.span() =>
                #ident.to_owned()
            }
        })
        .collect()
}
