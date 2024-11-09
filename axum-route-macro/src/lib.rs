#![feature(proc_macro_diagnostic)]
#![feature(proc_macro_def_site)]
#![feature(proc_macro_span)]
#![feature(proc_macro_hygiene)]

use quote::quote;
use syn::{parse_macro_input, FnArg, ItemFn, Meta, MetaList, ReturnType};
use proc_macro::{TokenStream, Span, Diagnostic, Level};
use proc_macro::MultiSpan;
use syn::parse::Parser;
use crate::route::RouteDef;
use syn::Type;
use quote::ToTokens;
use syn::GenericArgument;
use syn::PathArguments;
use std::collections::HashMap;
use std::marker::PhantomData;
use syn::ItemUse;
use syn::visit::Visit;
use syn::UseTree;
use std::sync::{Arc, Mutex};
use once_cell::sync::Lazy;
use syn::ItemMod;
use proc_macro2::Ident;
use std::collections::HashSet;

mod route;

static USE_COLLECTOR: Lazy<Mutex<Vec<String>>> = Lazy::new(|| {
    Mutex::new(Vec::new())
});

#[derive(Debug)]
struct UseCollector {
    uses: Vec<String>,
}

fn uses_to_map(use_statements: Vec<String>) -> HashMap<String, String> {
    let mut result_map = HashMap::new();

    for statement in use_statements {
        // remove the trailing ;
        let statement = statement.trim_end_matches(';');

        // make sure starts with `use`
        if !statement.starts_with("use ") {
            continue;
        }

        // get path only
        let path_part = &statement[4..]; // remove "use "
        let (full_path, items) = if path_part.contains('{') {
            let parts: Vec<&str> = path_part.splitn(2, '{').collect();
            let full_path = parts[0].trim().to_string();
            let items = parts[1].trim_end_matches('}').trim().to_string();
            (full_path, items)
        } else {
            (path_part.trim().to_string(), String::new())
        };

        // process items
        if !items.is_empty() {
            let items_list: Vec<&str> = items.split(',').collect();
            for item in items_list {
                let trimmed_item = item.trim();
                let entry_path = format!("{}{}", full_path, trimmed_item).replace(" ","");
                if trimmed_item.contains("::") {
                    // identity contains ::
                    let trimmed_item = trimmed_item.split("::").last().unwrap().trim();
                    result_map.insert(trimmed_item.trim().to_string(), entry_path);
                } else {
                    // common identity
                    result_map.insert(trimmed_item.trim().to_string(), entry_path);
                }
            }
        } else {
            // get the last segment as key
            let segments: Vec<&str> = full_path.split("::").collect();
            if let Some(last_segment) = segments.last() {
                let entry_path = format!("{}", full_path.replace(" ",""));
                result_map.insert(last_segment.trim().to_string(), entry_path);
            }
        }
    }

    result_map
}

impl<'ast> Visit<'ast> for UseCollector {
    fn visit_item_use(&mut self, node: &'ast ItemUse) {
        let full_path = node.to_token_stream().to_string(); // Capture the full path first
        self.uses.push(full_path);
        // Continue visiting the node's children
        syn::visit::visit_item_use(self, node);
    }
}

/// A macro to parse the `use` statements of a module.
/// For this to work, you must define a module with code immediately following.
/// For example:
///
/// ```rust
/// pub mod a_mod {
///     // Code for a_mod goes here...
/// }
/// ```
#[proc_macro_attribute]
pub fn handlers(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // clear the collector
    USE_COLLECTOR.lock().unwrap().clear();

    // parse the mod's content
    let input = parse_macro_input!(item as ItemMod);

    let mut collector = UseCollector {
        uses: Vec::new(),
    };

    collector.visit_item_mod(&input);

    //Store the collected use statements in the static USE_COLLECTOR
    let mut collector_lock = USE_COLLECTOR.lock().unwrap();
    collector_lock.extend(collector.uses.clone());

    // return the orginal mod
    quote! {
        #input
    }.into()
}

fn filter_use_statements(
    collected_uses_map: HashMap<String, String>,
    fn_args: Vec<String>,
    fn_return_type: String,
) -> Vec<String> {
    let mut used_types = HashSet::<String>::new();

    // Function to extract types from a given string
    let mut extract_types = |s: &str| {
        // Use a regex to find types by matching alphanumeric characters and "::" for paths
        let re = regex::Regex::new(r"\b[a-zA-Z_][a-zA-Z0-9_]*(?:::[a-zA-Z_][a-zA-Z0-9_]*)*\b").unwrap();
        for cap in re.captures_iter(s) {
            if let Some(matched) = cap.get(0) {
                used_types.insert(matched.as_str().to_string());
            }
        }
    };

    // Extract types from function arguments
    for arg in fn_args {
        extract_types(&arg);
    }

    // Extract types from return type
    extract_types(&fn_return_type);

    // Collect the unique use statements based on the types identified
    used_types
        .iter()
        .filter_map(|type_name| collected_uses_map.get(type_name))
        .cloned()
        .collect()
}

#[proc_macro_attribute]
pub fn route(attr: TokenStream, item: TokenStream) -> TokenStream {
    let ic = item.clone();
    let input_fn = parse_macro_input!(ic as ItemFn);
    let fn_name = &input_fn.sig.ident;
    // Emit a warning during compilation
    let func_span = fn_name.span(); // Get the span of the function name

    let mut msgs:Vec<String> = vec![];
    msgs.push("==================================".to_string());
    msgs.push(format!("fn name {}",fn_name));

    let span = Span::call_site();
    let source = span.source_file();
    msgs.push(format!("file path {}",source.path().to_str().unwrap()));

    //extract the route's method and path
    let routeDef:RouteDef = match syn::parse(attr) {
        Ok(args) => args,
        Err(err) => return err.into_compile_error().into(),
    };

    msgs.push(format!("attr parsed routeDef {:#?}",routeDef));

    // gain a USE_COLLECTOR lock
    let collector_lock = USE_COLLECTOR.lock().unwrap();

    // from USE_COLLECTOR clone the `uses` statements and convert into a map
    let collected_uses_map = uses_to_map(collector_lock.clone());

    // Collect argument types and names
    let fn_args: Vec<String> = input_fn.sig.inputs.iter().filter_map(|arg| {
        if let FnArg::Typed(pat_type) = arg {
            //let arg_type = get_fully_qualified_type(&pat_type.ty, &HashMap::new());
            // Get the name of the argument
            let arg_name = quote! { #pat_type }.to_string().replace(" ","");
            Some(arg_name)
        } else {
            None
        }
    }).collect();

    // Get the return type
    let fn_return_type = match &input_fn.sig.output {
        ReturnType::Type(_, ty) => quote! { #ty }.to_string().replace(" ",""),//get_fully_qualified_type(ty, &HashMap::new()),
        ReturnType::Default => "()".to_string(),
    };

    let use_statements = filter_use_statements(collected_uses_map.clone(),fn_args.clone(),fn_return_type.clone());

    // Prepare message output
    msgs.push(format!("fn args {:#?}, return type {:#?}, use_statements {:#?}", fn_args, fn_return_type,use_statements));

    /*let ast = match syn::parse::<syn::ItemFn>(item.clone()) {
        Ok(ast) => ast,
        Err(err) => return err.into_compile_error().into(),
    };*/

    let path = routeDef.path;
    let httpd_method = routeDef.method;
    let fn_name = fn_name.to_string();
    let fn_args = fn_args.join(";");
    let use_statements = use_statements.join(";");
    let dynamic_struct_name = Ident::new(&format!("RouteProvider{}",&fn_name), proc_macro2::Span::call_site());
    let method_ident = Ident::new(&format!("{}",&httpd_method), proc_macro2::Span::call_site());
    let handler_ident = Ident::new(&format!("{}",&fn_name), proc_macro2::Span::call_site());

    // Generate the FnInfo struct
    let expanded = quote! {
        #input_fn // Keep the original function

        #[allow(non_camel_case_types, missing_docs)]
        pub struct #dynamic_struct_name;

        // Implement RouteProvider for struct #name
        impl axum_route_helper::RouteProvider for #dynamic_struct_name {
            fn add_route(&self, router: axum::Router) -> axum::Router {
                router.route(#path,axum::routing::#method_ident(#handler_ident))
            }
            fn get_route(&self) -> axum_route_helper::RouteMethodDesc {
                axum_route_helper::RouteMethodDesc::new(#path.to_string(),#httpd_method.to_string(),#fn_name.to_string(),#fn_args.to_string(),#fn_return_type.to_string(),#use_statements.to_string())
            }
        }

        axum_route_helper::register_route_provider!(#dynamic_struct_name);
    };

    msgs.push("==================================".to_string());
    // 创建一个help
    let mut diagnostic = Diagnostic::new(Level::Help, msgs.join("\n"));
    diagnostic.set_spans(vec![Span::def_site()]);
    diagnostic.emit(); // 发出Help

    TokenStream::from(expanded)
}


