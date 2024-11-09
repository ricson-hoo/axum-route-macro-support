use axum::Router;
use axum::routing::{get};

pub trait RouteProvider: Send + Sync + 'static{
    fn add_route(&self,route: Router) -> Router;
    fn get_route(&self) -> RouteMethodDesc;
}


#[derive(Debug,Clone)]
pub struct RouteMethodDesc {
    pub path: String,
    pub http_method: String,
    pub fn_name: String,
    pub fn_args:String,
    pub fn_return_type:String,
    pub use_statements:String,
}

impl RouteMethodDesc {
    pub fn new(path: String,http_method: String,fn_name: String,fn_args:String,fn_return_type:String,use_statements:String)->Self{
        RouteMethodDesc {
            path,
            http_method,
            fn_name,
            fn_args,
            fn_return_type,
            use_statements
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

/*
 add routes
 */
pub fn add_routes(router: Router) -> Router {
    let mut router = router;
    for route_provider in inventory::iter::<&dyn RouteProvider>{
        router = route_provider.add_route(router);
    }
    router
}

/*
get routes description
 */
pub fn get_routes_desc() -> Vec<RouteMethodDesc> {
    let mut route_method_descs:Vec<RouteMethodDesc> = vec![];
    for route_provider in inventory::iter::<&dyn RouteProvider>{
        route_method_descs.push(route_provider.get_route());
    }
    route_method_descs
}

