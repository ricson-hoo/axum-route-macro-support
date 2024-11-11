use std::collections::HashMap;
use std::fs::File;
use std::{fs, io};
use std::path::Path;
use axum::Router;
use axum::routing::{get};
use std::io::Write;

pub trait RouteProvider: Send + Sync + 'static{
    fn add_route(&self,route: Router) -> Router;
    fn get_route(&self) -> RouteMethodDesc;
}


#[derive(Debug,Clone)]
pub struct RouteMethodDesc {
    pub mod_name:String,
    pub path: String,
    pub http_method: String,
    pub fn_name: String,
    pub fn_args:String,
    pub fn_return_type:String,
    pub use_statements:String,
}

impl RouteMethodDesc {
    pub fn new(mod_name:String, path: String,http_method: String,fn_name: String,fn_args:String,fn_return_type:String,use_statements:String)->Self{
        RouteMethodDesc {
            mod_name,
            path,
            http_method,
            fn_name,
            fn_args,
            fn_return_type,
            use_statements
        }
    }
}

///how the param value is being provided
#[derive(Debug,Clone,PartialEq)]
pub enum FnArgValueForm{
    Json,Path,QueryString
}

#[derive(Debug,Clone)]
pub struct FnArgInfo {
    pub name:String,
    pub arg_type:String,//String,i32,Product,User, etc.
    pub value_form:FnArgValueForm,//Json,Path,QueryString, where QueryString is the default
}

///Api clients code generation configuration
pub struct ApiClientCodeGenConf {
    pub output_dir:String,
    pub http_client_path:String,
    pub api_error_path:String,
    pub response_wrapper_path:String
}

impl ApiClientCodeGenConf {
    pub fn new(output_dir:String,http_client_path:String,api_error_path:String,response_wrapper_path:String)->Self{
        ApiClientCodeGenConf{
             output_dir,
             http_client_path,
             api_error_path,
             response_wrapper_path
        }
    }
}

#[macro_export]
macro_rules! register_route_provider {
    ($ty:ident) => {
        inventory::submit! {
            &$ty as &dyn axum_route_helper::RouteProvider
        }
    };
}

inventory::collect!(&'static dyn RouteProvider);

///add routes
pub fn add_routes(router: Router) -> Router {
    let mut router = router;
    for route_provider in inventory::iter::<&dyn RouteProvider>{
        router = route_provider.add_route(router);
    }
    router
}

///get routes description
pub fn get_routes_desc() -> Vec<RouteMethodDesc> {
    let mut route_method_descs:Vec<RouteMethodDesc> = vec![];
    for route_provider in inventory::iter::<&dyn RouteProvider>{
        route_method_descs.push(route_provider.get_route());
    }
    route_method_descs
}

/// Generate API clients code
pub fn generate_api_client(conf:ApiClientCodeGenConf) -> io::Result<()>{

    let output_dir = conf.output_dir;
    let http_client_path = conf.http_client_path;
    let api_error_path = conf.api_error_path;
    let response_wrapper_path = conf.response_wrapper_path;

    let dir_path = Path::new(&output_dir);
    prepare_directory(dir_path.clone());
    let routes = get_routes_desc();
    // Group routes by mod_name
    let mut grouped_routes: HashMap<String, Vec<RouteMethodDesc>> = HashMap::new();
    for route in routes {
        grouped_routes.entry(route.mod_name.clone()).or_default().push(route);
    }
    let skip_statements = vec!["axum::Json","axum::extract::Path","axum::extract::Query"];
    // Iterate over each group and generate the corresponding file
    for (mod_name, method_descs) in grouped_routes {
        // Create the output file path
        let file_name = format!("{}_api_client.rs", mod_name);
        let file_path = dir_path.join(file_name);
        let mut file = File::create(&file_path)?;

        // Write distinct use statements
        let use_statements = method_descs.first().map_or("", |desc| &desc.use_statements);

        let statements: Vec<String> = use_statements.split(';')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty() && !skip_statements.contains(s))
            .map(|s| format!("use {};", s))
            .collect();

        writeln!(file, "use {};", http_client_path)?;
        writeln!(file, "use {};", api_error_path)?;
        writeln!(file, "use {};", response_wrapper_path)?;
        for statement in statements {
            writeln!(file, "{}", statement)?;
        }

        writeln!(file, "\n")?;

        // Generate API client functions
        for desc in method_descs {
            let fn_code = generate_fn_code(&desc);
            writeln!(file, "{}\n", fn_code)?;
        }
    }
    Ok(())
}

pub fn prepare_directory(path:& std::path::Path){
    if !path.exists() {
        fs::create_dir_all(path).expect(&format!("Failed to create directory {}", path.display()));
    }
}

/// Generate the API client function code for a given RouteMethodDesc
///
/// desc RouteMethodDesc {
///     mod_name: "product",
///     path: "/api/product/save",
///    http_method: "post",
///     fn_name: "save_product",
///     fn_args: "Json(product):Json<Product>",
///     fn_return_type: "Json<ApiResponse<Product>>",
///     use_statements: "shared::request::response::ApiResponse;shared::entity::Product;axum::Json",
/// }
/// desc RouteMethodDesc {
///     mod_name: "product",
///     path: "/api/product/{id}/{action}",
///     http_method: "get",
///     fn_name: "get_product",
///     fn_args: "Path((id,action)):Path<(String,String)>",
///     fn_return_type: "Json<ApiResponse<Product>>",
///     use_statements: "axum::Json;shared::request::response::ApiResponse;shared::entity::Product;axum::extract::Path",
/// }
///
fn generate_fn_code(desc: &RouteMethodDesc) -> String {
    println!("desc {:#?}", desc);

    let fn_name = &desc.fn_name;
    let path = &desc.path;
    let http_method = desc.http_method.to_lowercase(); // Ensure the method is in lowercase
    let fn_return_type = &desc.fn_return_type;

    // Process the function arguments
    let fn_args_info = parts_fn_args_names_and_types(desc.fn_args.clone());

    println!("fn_args_info = {:#?}",fn_args_info);

    let fn_args: String = fn_args_info.iter()
        .map(|item| format!("{}: {}", item.name, item.arg_type))
        .collect::<Vec<String>>() // Collect to a Vec<String>
        .join(", "); // Join the Vec<String> into a single String with ", " as separator

    // Clean up the return type
    let fn_return_type = if fn_return_type.starts_with("Json<") {
        // Extract the content inside Json<>
        let start_index = fn_return_type.find('<').unwrap() + 1;
        let end_index = fn_return_type.rfind('>').unwrap();

        // Extract content
        let inner_content = &fn_return_type[start_index..end_index];

        inner_content.to_string()
    } else {
        // If it doesn't start with Json<, return the original type or handle accordingly
        fn_return_type.to_string()
    };

    let mut fn_return_data_type = fn_return_type.clone();
    if fn_return_data_type.starts_with("ApiResponse<") {
        // Remove "ApiResponse<" and the last ">"
        fn_return_data_type = fn_return_data_type.trim_start_matches("ApiResponse<").trim_end_matches('>').to_string();
    } else if fn_return_data_type.starts_with("PagingResponse<") {
        // Remove "PagingResponse<" and the last ">"
        fn_return_data_type = fn_return_data_type.trim_start_matches("PagingResponse<").trim_end_matches('>').to_string();
    }

    let path = if path.contains("{") { //e.g., /api/product/{id}/{action}
        //should be format!("/api/product/{}/{}",id,action)
        path.to_string()
    }else {
        path.to_string()
    };

    format!(
        r#"pub async fn {fn_name}({fn_args}) -> Result<{fn_return_data_type}, ApiError> {{
        let result = HttpClient::{}.await?;
        Ok(result)
    }}"#,
        // Generate the vector of arguments for the HttpClient call
        //generate_http_client_method(&http_method, fn_args_info.iter().any(|it|it.value_form == FnArgValueForm::Json)),
        generate_http_client_call(http_method,path, fn_args_info, fn_return_type.clone())
    )
}

/*fn generate_http_client_method(http_method: &str,has_json:bool) -> String {
    if http_method == "post" && has_json{
        "post_body".to_string()
    }else {
        http_method.to_string()
    }
}*/

fn generate_http_client_call(http_method:String, mut path:String, fn_args_info:Vec<FnArgInfo>, fn_return_type:String) -> String {
    let mut path_params:Vec<String> = vec![];
    let mut query_string_params:Vec<String> = vec![];
    let mut body:Option<FnArgInfo> = None;
    //method
    let mut method = http_method.to_lowercase();
    let is_api_response = fn_return_type.contains("ApiResponse");
    let is_paging_response = fn_return_type.contains("PagingResponse");
    if is_paging_response {
        method.push_str("_paging");
    }

    //path
    fn_args_info.iter().for_each(|it|{
        match it.value_form {
            FnArgValueForm::Json => {
                if body.is_none() {
                    body = Some(it.clone());
                }
            },
            FnArgValueForm::Path => {
                let pattern = format!("{{{}}}", it.name);
                path = path.replace(&pattern,"{}");
                path_params.push(it.name.clone());
            },
            _ => {
                query_string_params.push(it.name.clone());
            }
        }
    });


    let mut http_client_call = format!("{}(", method);

    if path_params.is_empty(){
        http_client_call.push_str(&format!("\"{}\"",path));
    }else {
        http_client_call.push_str(&format!("&format!(\"{}\",{})",path,path_params.join(", ")));
    }

    //post body
    if method == "post" {
        if body.is_some() {
            http_client_call.push_str(", &");
            http_client_call.push_str(&format!("Some({})",body.unwrap().name.as_str()));
        }else {
            http_client_call.push_str(", &Option::<i8>::None");
        }
    }

    //params
    if query_string_params.len() > 0 {
        http_client_call.push_str(", vec![");
        query_string_params.iter().for_each(|it|{
            http_client_call.push_str(&format!("(\"{}\",{})",&it,&it));
        });
        http_client_call.push_str("]");
    }else {
        http_client_call.push_str(", vec![]");
    }

    //wrapper_type
    if !is_paging_response{
        if is_api_response {
            http_client_call.push_str(", ResponseWrapper::ApiResponse");
        }else {
            http_client_call.push_str(", ResponseWrapper::Nothing");
        }
    }
    http_client_call.push_str(")");

    http_client_call
}

/// Generate the argument list for the HttpClient call
fn generate_http_client_args(fn_args: &str) -> String {
    fn_args
        .split(',')
        .map(|arg| {
            let trimmed = arg.trim();
            if trimmed.is_empty() {
                return String::new();
            }
            let parts: Vec<&str> = trimmed.split(':').collect();
            if parts.len() != 2 {
                return String::new(); // Handle unexpected formats
            }
            let arg_name = parts[0].trim();
            format!(r#"("{}",{})"#, arg_name, arg_name) // Expecting arg names to match the values
        })
        .filter(|s| !s.is_empty()) // Remove empty strings
        .collect::<Vec<String>>()
        .join(",")
}


///解析方法参数名、类型
fn parts_fn_args_names_and_types(fn_args: String) -> Vec<FnArgInfo> {
    let mut fn_args_info: Vec<FnArgInfo> = vec![];
    let fn_args_names_and_types: Vec<&str> = fn_args.split(':').collect();

    if fn_args_names_and_types.len() == 2 { // Only two are acceptable
        let names_str = fn_args_names_and_types[0].trim(); // e.g., "Json(product)" or "Path((id,action))"
        let types_str = fn_args_names_and_types[1].trim(); // e.g., "Json<Product>" or "Path<(String,String)>"

        let mut value_form = FnArgValueForm::QueryString; //default

        // Clean up names_str
        let cleaned_names_str = if names_str.starts_with("Json(") {
            value_form = FnArgValueForm::Json;
            &names_str[5..names_str.len() - 1] // Remove "Json(" prefix and ")" suffix
        } else if names_str.starts_with("Path(") {
            value_form = FnArgValueForm::Path;
            let inner = &names_str[5..names_str.len() - 1]; // Remove "Path(" prefix and ")" suffix
            if inner.starts_with('(') && inner.ends_with(')') {
                &inner[1..inner.len() - 1] // Remove the outer parentheses
            } else {
                inner // Return as is if there's no outer parentheses
            }
        } else {
            value_form = FnArgValueForm::QueryString;
            names_str // Return as is if it doesn't match known formats
        };

        // Process names
        let names: Vec<String> = cleaned_names_str.split(",").map(|p| p.trim().to_string()).collect();

        // Clean up types_str
        let cleaned_types_str = if types_str.starts_with("Json<") {
            &types_str[5..types_str.len() - 1] // Remove "Json<" and ">"
        } else if types_str.starts_with("Path<") {
            let inner = &types_str[5..types_str.len() - 1]; // Remove "Path<" and ">"
            if inner.starts_with('(') && inner.ends_with(')') {
                &inner[1..inner.len() - 1] // Remove the outer parentheses
            } else {
                inner // Return as is if no outer parentheses
            }
        } else {
            types_str // Return as is if it doesn't match known formats
        };

        // Process types
        let types: Vec<String> = cleaned_types_str.split(",").map(|s| s.trim().to_string()).collect();

        // Create FnArgInfo for each name/type pair
        for (index, name) in names.iter().enumerate() {
            fn_args_info.push(FnArgInfo {
                name: name.to_string(),
                arg_type: types[index].clone(),
                value_form: value_form.clone(),
            });
        }
    }

    fn_args_info
}
/*fn parts_fn_args_names_and_types(fn_args:String) -> (Vec<String>,Vec<String>) {
    let mut names:Vec<String> = vec![];
    let mut types:Vec<String> = vec![];
    let fn_args_names_and_types:Vec<&str> = fn_args.split(':').collect();
    if fn_args_names_and_types.len() == 2 { //only two are acceptable
        let names_str = fn_args_names_and_types[0].trim(); //Json(product) 或 Path((id,action))
        // Check if the string starts with "Json(" or "Path("
        let cleaned_names_str = if names_str.starts_with("Json(") {
            &names_str[5..names_str.len() - 1] // Remove "Json(" prefix and ")" suffix
        } else if names_str.starts_with("Path(") {
            // For Path, we need to handle the double parentheses case
            let inner = &names_str[5..names_str.len() - 1]; // Remove "Path(" prefix and ")" suffix
            if inner.starts_with('(') && inner.ends_with(')') {
                &inner[1..inner.len() - 1] // Remove the outer parentheses
            } else {
                inner // Return as is if there's no outer parentheses
            }
        } else {
            names_str // Return as is if it doesn't match known formats
        };
        names = cleaned_names_str.split(",").map(|p|p.to_string()).collect();

        let types_str = fn_args_names_and_types[1].trim(); // e.g., "Json<Product>" or "Path<(String,String)>"

        // Clean types_str to extract the inner content
        let cleaned_types_str = if types_str.starts_with("Json<") {
            &types_str[5..types_str.len() - 1] // Remove "Json<" and ">"
        } else if types_str.starts_with("Path<") {
            let inner = &types_str[5..types_str.len() - 1]; // Remove "Path<" and ">"
            if inner.starts_with('(') && inner.ends_with(')') {
                &inner[1..inner.len() - 1] // Remove the outer parentheses
            } else {
                inner // Return as is if no outer parentheses
            }
        } else {
            types_str // Return as is if it doesn't match known formats
        };

        // Split cleaned types into the types vector
        types = cleaned_types_str.split(",").map(|s| s.trim().to_string()).collect();
    }
    (names,types)
}*/
