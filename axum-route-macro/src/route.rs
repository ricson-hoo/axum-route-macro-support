use proc_macro::TokenStream;
use std::collections::HashMap;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{quote, ToTokens};
use syn::{punctuated::Punctuated, LitStr, Path, Token};
use syn::spanned::Spanned;
use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::Mutex;

/*#[derive(Debug)]
pub struct RouteOption{
    pub name:String,
    pub value:String,
}
impl RouteOption{
    pub fn new(name:String,value:String)->Self{
        RouteOption{name,value}
    }
}*/

#[derive(Debug)]
pub struct RouteDef {
    pub path: String,
    pub method: String,
    pub options: HashMap<String,String>,
}

impl syn::parse::Parse for RouteDef {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        // Parse the route path as a String
        let path = input.parse::<syn::LitStr>().map_err(|mut e| {
            e.combine(syn::Error::new(
                e.span(),
                r#"Failed to parse route definition, expected #[route("<path>",method=\"get/post/...\")]"#,
            ));
            e
        })?.value();// Convert LitStr to String

        let possible_methods = vec!["get".to_string(), "post".to_string(), "put".to_string(),"delete".to_string(),"head".to_string(),"options".to_string(),"trace".to_string(),"patch".to_string()];

        let mut method = "".to_string();
        let mut options = HashMap::new();

        // Check for the next token
        let next_token: Result<Token![,], _> = input.parse();
        match next_token {
            Ok(_) => {
                if input.cursor().literal().is_some() {
                    return Err(syn::Error::new(
                        Span::call_site(),
                        r#"Route options were expected, like method=\"get\", but a literal was given."#,
                    ));
                }
                while !input.is_empty() {
                    let meta_name_value: syn::MetaNameValue = input.parse()?;
                    // `value` is of type `syn::Expr`, so we need to match on it directly.
                    if let syn::Expr::Lit(lit) = meta_name_value.value {
                        if let syn::Lit::Str(lit_str) = lit.lit {
                            let meta_name = meta_name_value.path.get_ident().unwrap().to_string();
                            let meta_value = lit_str.value();
                            if meta_name == "method".to_string() {
                                method = meta_value;
                            }else{
                                options.insert(meta_name, meta_value);
                            }
                        }
                    } else {
                        return Err(syn::Error::new(
                            meta_name_value.span(),
                            "Expected a string literal for the option value.",
                        ));
                    }

                    // Check for a comma to continue parsing more options
                    if input.peek(Token![,]) {
                        input.parse::<Token![,]>()?;
                    } else {
                        break; // Exit the loop if no more commas
                    }
                }
            },
            Err(_) => {
                // If there's no following token, we return an empty options array
                return Ok(Self {
                    path,
                    method,
                    options: HashMap::new(),
                });
            },
        };
        Ok(Self { path, method, options })
    }
}

/*#[derive(Debug)]
pub(crate) struct RouteDef {
    pub(crate) route_path: LitStr,
    pub(crate) options: Punctuated<syn::MetaNameValue, Token![,]>,
}

impl syn::parse::Parse for crate::route::RouteDef {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let route_path = input.parse::<syn::LitStr>().map_err(|mut e| {
            e.combine(syn::Error::new(
                e.span(),
                r#"Failed to parse route definition, expected #[route("<path>",method=\"get/post/...\")]"#,
            ));
            e
        })?;


        let next_token: Result<Token![,], _> = input.parse();
        match next_token {
            Ok(_) => {
                if input.cursor().literal().is_some() {
                    return Err(syn::Error::new(
                        Span::call_site(),
                        r#"Route options were expected, like method=\"get\", but a literal was given."#,
                    ));
                }
                let options = input.parse_terminated(syn::MetaNameValue::parse, Token![,])?;
                return Ok(Self { route_path, options })
            },
            Err(_) => {
                return Ok(Self {
                    route_path,
                    options: Punctuated::new(),
                });
            },
        };
    }
}*/

