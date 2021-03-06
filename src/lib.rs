//! This is a simple yet expressive router for http requests, abstract enough to be used with any http library on stable Rust.
//!
//! ### Key features:
//! - Very expressive routes with fully typed parameters
//! - Can be used with any http lib
//! - Few dependencies (only `regex` and `lazy_static`)
//!
//! ### Getting started (for Hyper >= 0.12)
//!
//! In your Cargo.toml
//!
//! ```toml
//! [dependencies]
//! http_router = "0.1"
//! ```
//!
//! In your lib.rs or main.rs:
//! ```rust
//! #[macro_use]
//! extern crate http_router;
//! ```
//!
//! In your struct than implements Hyper `Service`:
//!
//! ```rust
//! // Each handler must have the same return type
//! // A good candidate might be a Box<Future<Item = hyper::Response, Error = Error>>
//! // The cost of this macro is next to zero, so it's ok to call it on each request
//! let router = router!(
//!     GET / => get_users,
//!
//!     GET /users => get_users,
//!     POST /users => post_users,
//!     PUT /users/{user_id: usize} => put_users,
//!     DELETE /users/{user_id: usize} => delete_users,
//!
//!     GET /users/{user_id: usize}/transactions => get_transactions,
//!     POST /users/{user_id: usize}/transactions => post_transactions,
//!     PUT /users/{user_id: usize}/transactions/{hash: String} => put_transactions,
//!     DELETE /users/{user_id: usize}/transactions/{hash: String} => delete_transactions,
//!
//!     _ => not_found,
//! );
//!
//! let path = req.uri.path();
//! let ctx = Context { ... };
//! // This will return a value of the matched handler's return type
//! // E.g. the aforementioned Box<Future<Item = hyper::Response, Error = Error>>
//! router(ctx, req.method.into(), path)
//! ```
//!
//! A file with handlers implementation
//!
//! ```rust
//! // Params from a route become handlers' typed params.
//! // If a param's type doesn't match (e.g. you supplied `sdf` as a user id, that must be `usize`)
//! // then this route counts as non-matching
//!
//! type ServerFuture = Box<Future<Item = hyper::Response, Error = Error>>;
//!
//! pub fn get_users(context: &Context) -> ServerFuture {
//!     ...
//! }
//!
//! pub fn post_users(context: &Context) -> ServerFuture {
//!     ...
//! }
//!
//! pub fn put_users(context: &Context, user_id: usize) -> ServerFuture {
//!     ...
//! }
//!
//! pub fn delete_users(context: &Context, id: usize) -> ServerFuture {
//!     ...
//! }
//!
//! pub fn get_transactions(context: &Context, user_id: usize) -> ServerFuture {
//!     ...
//! }
//!
//! pub fn post_transactions(context: &Context, user_id: usize) -> ServerFuture {
//!     ...
//! }
//!
//! pub fn put_transactions(context: &Context, user_id: usize, hash: String) -> ServerFuture {
//!     ...
//! }
//!
//! pub fn delete_transactions(context: &Context, user_id: usize, hash: String) -> ServerFuture {
//!     ...
//! }
//!
//! pub fn not_found(_context: &Context) -> ServerFuture {
//!     ...
//! }
//!
//! ```
//!
//! See [examples folder](https://github.com/alleycat-at-git/http_router/tree/master/examples/hyper_example) for a complete Hyper example
//!
//! ### Using with other http libs
//!
//! By default this crate is configured to be used with `hyper >=0.12`. If you want to use it with other libs, you might want to opt out of default features for this crate. So in your Cargo.toml:
//!
//! ```toml
//! [dependencies]
//! http_router = config = { version = "0.1", default-features = false}
//! ```
//!
//! The `router!` macro is independent of any framework. However, it returns a closure that takes 3 params - `context`, `method` and `path`. You need to supply these 3 params from your http lib.
//!
//! `context` is a param of your user-defined type. e.g. `Context`. It will be passed as a first argument to all of your handlers. You can put there any values like database interfaces and http clients as you like.
//!
//! `method` is a param of type Method defined in `http_router` lib. It is one of `GET`, `POST`, etc.
//!
//! `path` is a `&str` which is the current route for a request.
//!
//! Once you define these 3 params, you can use the `router!` macro for routing.
//!
//! ### Benchmarks
//!
//! Right now the router with 10 routes takes approx 50 microseconds per route
//!

extern crate regex;
#[macro_use]
extern crate lazy_static;
#[cfg(feature = "with_hyper")]
extern crate hyper;

mod method;

pub use self::method::Method;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

lazy_static! {
    static ref REGEXES: Arc<Mutex<HashMap<String, regex::Regex>>> =
        { Arc::new(Mutex::new(HashMap::new())) };
}

/// This is an implementation detail and *should not* be called directly!
#[doc(hidden)]
pub fn __http_router_create_regex(s: &str) -> regex::Regex {
    let mut _result: Option<regex::Regex> = None;
    {
        let regexes = REGEXES.lock().expect("Failed to obtain mutex lock");
        _result = regexes.get(s).cloned();
    };
    _result.unwrap_or_else(|| {
        let re = regex::Regex::new(s).unwrap();
        let mut regexes = REGEXES.lock().expect("Failed to obtain mutex lock");
        regexes.insert(s.to_string(), re.clone());
        re
    })
}

/// This macro returns a closure that takes 3 params. See crate doc for more details.
///
/// ### Limitations:
/// - Home route is optional and should come first
/// - Fallback route (`_`) is required and should come last
///
/// ### Performace
/// Macro routers itself has almost no cost, so you can call it
/// everywhere as many times as you like. The closure that it returns
/// have some cost (approx 50 microseconds per one call).
///
/// ### Thread safety
/// The closure returned by this macro is thread-safe.
#[macro_export]
macro_rules! router {
    // convert params from string
    (@parse_type $value:expr, $ty:ty) => {{
        let maybe_val = $value.parse::<$ty>();
        if maybe_val.is_err() { return None };
        maybe_val.unwrap()
    }};

    // call handler with params
    (@call_pure $context:expr, $handler:ident, $params:expr, $({$id:ident : $ty:ty : $idx:expr}),*) => {{
        $handler(&$context, $({
            let value = $params[$idx];
            router!(@parse_type value, $ty)
        }),*)
    }};

    // Extract params from route, 0 params case
    (@call, $context:expr, $handler:ident, $params:expr, $($p:ident)*) => {{
        $handler(&$context)
    }};

    // Extract params from route, 1 params case
    (@call, $context:expr, $handler:ident, $params:expr, $($p:ident)* {$id1:ident : $ty1:ty} $($p1:ident)*) => {{
        router!(@call_pure $context, $handler, $params, {$id1 : $ty1 : 0})
    }};

    // Extract params from route, 2 params case
    (@call, $context:expr, $handler:ident, $params:expr, $($p:ident)* {$id1:ident : $ty1:ty} $($p1:ident)* {$id2:ident : $ty2:ty} $($p2:ident)*) => {{
        router!(@call_pure $context, $handler, $params, {$id1 : $ty1 : 0}, {$id2 : $ty2 : 1})
    }};

    // Extract params from route, 3 params case
    (@call, $context:expr, $handler:ident, $params:expr, $($p:ident)* {$id1:ident : $ty1:ty} $($p1:ident)* {$id2:ident : $ty2:ty} $($p2:ident)* {$id3:ident : $ty3:ty} $($p3:ident)*) => {{
        router!(@call_pure $context, $handler, $params, {$id1 : $ty1 : 0}, {$id2 : $ty2 : 1}, {$id3 : $ty3 : 2})
    }};

    // Extract params from route, 4 params case
    (@call, $context:expr, $handler:ident, $params:expr, $($p:ident)* {$id1:ident : $ty1:ty} $($p1:ident)* {$id2:ident : $ty2:ty} $($p2:ident)* {$id3:ident : $ty3:ty} $($p3:ident)* {$id4:ident : $ty4:ty} $($p4:ident)*) => {{
        router!(@call_pure $context, $handler, $params, {$id1 : $ty1 : 0}, {$id2 : $ty2 : 1}, {$id3 : $ty3 : 2}, {$id4 : $ty4 : 3})
    }};

    // Extract params from route, 5 params case
    (@call, $context:expr, $handler:ident, $params:expr, $($p:ident)* {$id1:ident : $ty1:ty} $($p1:ident)* {$id2:ident : $ty2:ty} $($p2:ident)* {$id3:ident : $ty3:ty} $($p3:ident)* {$id4:ident : $ty4:ty} $($p4:ident)* {$id5:ident : $ty5:ty} $($p5:ident)*) => {{
        router!(@call_pure $context, $handler, $params, {$id1 : $ty1 : 0}, {$id2 : $ty2 : 1}, {$id3 : $ty3 : 2}, {$id4 : $ty4 : 3}, {$id5 : $ty5 : 4})
    }};

    // Extract params from route, 6 params case
    (@call, $context:expr, $handler:ident, $params:expr, $($p:ident)* {$id1:ident : $ty1:ty} $($p1:ident)* {$id2:ident : $ty2:ty} $($p2:ident)* {$id3:ident : $ty3:ty} $($p3:ident)* {$id4:ident : $ty4:ty} $($p4:ident)* {$id5:ident : $ty5:ty} $($p5:ident)* {$id6:ident : $ty6:ty} $($p6:ident)*) => {{
        router!(@call_pure $context, $handler, $params, {$id1 : $ty1 : 0}, {$id2 : $ty2 : 1}, {$id3 : $ty3 : 2}, {$id4 : $ty4 : 3}, {$id5 : $ty5 : 4}, {$id6 : $ty6 : 5})
    }};

    // Extract params from route, 7 params case
    (@call, $context:expr, $handler:ident, $params:expr, $($p:ident)* {$id1:ident : $ty1:ty} $($p1:ident)* {$id2:ident : $ty2:ty} $($p2:ident)* {$id3:ident : $ty3:ty} $($p3:ident)* {$id4:ident : $ty4:ty} $($p4:ident)* {$id5:ident : $ty5:ty} $($p5:ident)* {$id6:ident : $ty6:ty} $($p6:ident)* {$id7:ident : $ty7:ty} $($p7:ident)*) => {{
        router!(@call_pure $context, $handler, $params, {$id1 : $ty1 : 0}, {$id2 : $ty2 : 1}, {$id3 : $ty3 : 2}, {$id4 : $ty4 : 3}, {$id5 : $ty5 : 4}, {$id6 : $ty6 : 5}, {$id6 : $ty6 : 6})
    }};

    // Test a particular route for match and forward to @call if there is match
    (@one_route_with_method $context:expr, $method:expr, $path:expr, $default:expr, $expected_method: expr, $handler:ident, $($path_segment:tt)*) => {{
        if $method != $expected_method { return None };
        let mut s = "^".to_string();
        $(
            s.push('/');
            let path_segment = stringify!($path_segment);
            if path_segment.starts_with('{') {
                s.push_str(r#"([\w-]+)"#);
            } else {
                s.push_str(path_segment);
            }
        )*
        // handle home case
        if s.len() == 1 { s.push('/') }
        s.push('$');
        let re = $crate::__http_router_create_regex(&s);
        if let Some(captures) = re.captures($path) {
            let _matches: Vec<&str> = captures.iter().skip(1).filter(|x| x.is_some()).map(|x| x.unwrap().as_str()).collect();
            Some(router!(@call, $context, $handler, _matches, $($path_segment)*))
        } else {
            None
        }
    }};

    // Transform GET token to Method::GET
    (@one_route $context:expr, $method:expr, $path:expr, $default:expr, GET, $handler:ident, $($path_segment:tt)*) => {
        router!(@one_route_with_method $context, $method, $path, $default, $crate::Method::GET, $handler, $($path_segment)*)
    };

    // Transform POST token to Method::POST
    (@one_route $context:expr, $method:expr, $path:expr, $default:expr, POST, $handler:ident, $($path_segment:tt)*) => {
        router!(@one_route_with_method $context, $method, $path, $default, $crate::Method::POST, $handler, $($path_segment)*)
    };
    // Transform PUT token to Method::PUT
    (@one_route $context:expr, $method:expr, $path:expr, $default:expr, PUT, $handler:ident, $($path_segment:tt)*) => {
        router!(@one_route_with_method $context, $method, $path, $default, $crate::Method::PUT, $handler, $($path_segment)*)
    };
    // Transform PATCH token to Method::PATCH
    (@one_route $context:expr, $method:expr, $path:expr, $default:expr, PATCH, $handler:ident, $($path_segment:tt)*) => {
        router!(@one_route_with_method $context, $method, $path, $default, $crate::Method::PATCH, $handler, $($path_segment)*)
    };
    // Transform DELETE token to Method::DELETE
    (@one_route $context:expr, $method:expr, $path:expr, $default:expr, DELETE, $handler:ident, $($path_segment:tt)*) => {
        router!(@one_route_with_method $context, $method, $path, $default, $crate::Method::DELETE, $handler, $($path_segment)*)
    };
    // Transform OPTIONS token to Method::OPTIONS
    (@one_route $context:expr, $method:expr, $path:expr, $default:expr, OPTIONS, $handler:ident, $($path_segment:tt)*) => {
        router!(@one_route_with_method $context, $method, $path, $default, $crate::Method::OPTIONS, $handler, $($path_segment)*)
    };

    // Transform HEAD token to Method::HEAD
    (@one_route $context:expr, $method:expr, $path:expr, $default:expr, HEAD, $handler:ident, $($path_segment:tt)*) => {
        router!(@one_route_with_method $context, $method, $path, $default, $crate::Method::HEAD, $handler, $($path_segment)*)
    };

    // Transform TRACE token to Method::TRACE
    (@one_route $context:expr, $method:expr, $path:expr, $default:expr, TRACE, $handler:ident, $($path_segment:tt)*) => {
        router!(@one_route_with_method $context, $method, $path, $default, $crate::Method::TRACE, $handler, $($path_segment)*)
    };

    // Transform CONNECT token to Method::CONNECT
    (@one_route $context:expr, $method:expr, $path:expr, $default:expr, CONNECT, $handler:ident, $($path_segment:tt)*) => {
        router!(@one_route_with_method $context, $method, $path, $default, $crate::Method::CONNECT, $handler, $($path_segment)*)
    };

    // Entry pattern
    ($($method_token:ident $(/$path_segment:tt)+ => $handler:ident,)* _ => $default:ident $(,)*) => {{
        move |context, method: $crate::Method, path: &str| {
            let mut result = None;
            $(
                if result.is_none() {
                    // we use closure here so that we could make early return from macros inside of it
                    let closure = || {
                        router!(@one_route context, method, path, $default, $method_token, $handler, $($path_segment)*)
                    };
                    result = closure();
                }
            )*
            result.unwrap_or_else(|| $default(&context))
        }
    }};

    // Entry pattern - with home first
    ($home_method_token:ident / => $home_handler:ident, $($method_token:ident $(/$path_segment:tt)+ => $handler:ident,)* _ => $default:ident $(,)*) => {{
        move |context, method: $crate::Method, path: &str| {
            let closure = || {
                router!(@one_route context, method, path, $default, $home_method_token, $home_handler,)
            };
            let mut result = closure();
            $(
                if result.is_none() {
                    // we use closure here so that we could make early return from macros inside of it
                    let closure = || {
                        router!(@one_route context, method, path, $default, $method_token, $handler, $($path_segment)*)
                    };
                    result = closure();
                }
            )*
            result.unwrap_or_else(|| $default(&context))
        }
    }};

    // Entry pattern - default only
    (_ => $default:ident $(,)*) => {
        |context, _method: $crate::Method, _path: &str| {
            $default(&context)
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate rand;

    // use self::test::Bencher;
    use super::*;
    use std::thread;

    const NUMBER_OF_THREADS_FOR_REAL_LIFE_TEST: usize = 4;
    const NUMBER_OF_TESTS_FOR_REAL_LIFE_TEST: usize = 3000;

    #[test]
    fn test_real_life() {
        let get_users = |_: &()| "get_users".to_string();
        let post_users = |_: &()| "post_users".to_string();
        let patch_users = |_: &(), id: u32| format!("patch_users({})", id);
        let delete_users = |_: &(), id: u32| format!("delete_users({})", id);
        let get_transactions = |_: &(), id: u32| format!("get_transactions({})", id);
        let post_transactions = |_: &(), id: u32| format!("post_transactions({})", id);
        let patch_transactions =
            |_: &(), id: u32, hash: String| format!("patch_transactions({}, {})", id, hash);
        let delete_transactions =
            |_: &(), id: u32, hash: String| format!("delete_transactions({}, {})", id, hash);
        let fallback = |_: &()| "404".to_string();

        let router = router!(
            GET / => get_users,
            GET /users => get_users,
            POST /users => post_users,
            PATCH /users/{user_id: u32} => patch_users,
            DELETE /users/{user_id: u32} => delete_users,
            GET /users/{user_id: u32}/transactions => get_transactions,
            POST /users/{user_id: u32}/transactions => post_transactions,
            PATCH /users/{user_id: u32}/transactions/{hash: String} => patch_transactions,
            DELETE /users/{user_id: u32}/transactions/{hash: String} => delete_transactions,
            _ => fallback,
        );
        let test_cases = [
            (Method::GET, "/", "get_users"),
            (Method::GET, "/users", "get_users"),
            (Method::POST, "/users", "post_users"),
            (Method::PATCH, "/users/12", "patch_users(12)"),
            (Method::DELETE, "/users/132134", "delete_users(132134)"),
            (
                Method::GET,
                "/users/534/transactions",
                "get_transactions(534)",
            ),
            (
                Method::POST,
                "/users/534/transactions",
                "post_transactions(534)",
            ),
            (
                Method::PATCH,
                "/users/534/transactions/0x234",
                "patch_transactions(534, 0x234)",
            ),
            (
                Method::DELETE,
                "/users/534/transactions/0x234",
                "delete_transactions(534, 0x234)",
            ),
            (Method::DELETE, "/users/5d34/transactions/0x234", "404"),
            (Method::POST, "/users/534/transactions/0x234", "404"),
            (Method::GET, "/u", "404"),
            (Method::POST, "/", "404"),
        ];
        for test_case in test_cases.into_iter() {
            let (method, path, expected) = test_case.clone();
            assert_eq!(router((), method.clone(), path), expected.to_string());
        }

        let mut threads: Vec<thread::JoinHandle<_>> = Vec::new();
        for _ in 0..NUMBER_OF_THREADS_FOR_REAL_LIFE_TEST {
            let handle = thread::spawn(move || {
                for _ in 0..NUMBER_OF_TESTS_FOR_REAL_LIFE_TEST {
                    let number = rand::random::<usize>() % test_cases.len();
                    let test_case = test_cases[number];
                    let (method, path, expected) = test_case;
                    assert_eq!(router((), method.clone(), path), expected.to_string());
                }
            });
            threads.push(handle);
        }
        for thread in threads {
            let _ = thread.join();
        }
    }

    #[allow(unused_mut)]
    #[test]
    fn test_home() {
        let get_home = |_: &()| "get_home";
        let unreachable = |_: &()| unreachable!();
        let router = router!(
            GET / => get_home,
            _ => unreachable
        );
        assert_eq!(router((), Method::GET, "/"), "get_home");
    }

    #[test]
    fn test_fallback() {
        let home = |_: &()| "home";
        let users = |_: &()| "users";
        let fallback = |_: &()| "fallback";
        let router = router!(
            GET / => home,
            POST /users => users,
            _ => fallback
        );
        assert_eq!(router((), Method::GET, "/"), "home");
        assert_eq!(router((), Method::POST, "/users"), "users");
        assert_eq!(router((), Method::GET, "/users"), "fallback");
        assert_eq!(router((), Method::GET, "/us"), "fallback");
        assert_eq!(router((), Method::PATCH, "/"), "fallback");
    }

    #[test]
    fn test_verbs() {
        let get_test = |_: &()| Method::GET;
        let post_test = |_: &()| Method::POST;
        let put_test = |_: &()| Method::PUT;
        let patch_test = |_: &()| Method::PATCH;
        let delete_test = |_: &()| Method::DELETE;
        let connect_test = |_: &()| Method::CONNECT;
        let options_test = |_: &()| Method::OPTIONS;
        let trace_test = |_: &()| Method::TRACE;
        let head_test = |_: &()| Method::HEAD;
        let panic_test = |_: &()| unreachable!();
        let router = router!(
            GET /users => get_test,
            POST /users => post_test,
            PUT /users => put_test,
            PATCH /users => patch_test,
            DELETE /users => delete_test,
            OPTIONS /users => options_test,
            CONNECT /users => connect_test,
            TRACE /users => trace_test,
            HEAD /users => head_test,
            _ => panic_test
        );

        assert_eq!(router((), Method::GET, "/users"), Method::GET);
        assert_eq!(router((), Method::POST, "/users"), Method::POST);
        assert_eq!(router((), Method::PUT, "/users"), Method::PUT);
        assert_eq!(router((), Method::PATCH, "/users"), Method::PATCH);
        assert_eq!(router((), Method::DELETE, "/users"), Method::DELETE);
        assert_eq!(router((), Method::OPTIONS, "/users"), Method::OPTIONS);
        assert_eq!(router((), Method::TRACE, "/users"), Method::TRACE);
        assert_eq!(router((), Method::CONNECT, "/users"), Method::CONNECT);
        assert_eq!(router((), Method::HEAD, "/users"), Method::HEAD);
    }

    #[test]
    fn test_params_number() {
        let zero = |_: &()| String::new();
        let one = |_: &(), p1: String| format!("{}", &p1);
        let two = |_: &(), p1: String, p2: String| format!("{}{}", &p1, &p2);
        let three = |_: &(), p1: String, p2: String, p3: String| format!("{}{}{}", &p1, &p2, &p3);
        let four = |_: &(), p1: String, p2: String, p3: String, p4: String| {
            format!("{}{}{}{}", &p1, &p2, &p3, &p4)
        };
        let five = |_: &(), p1: String, p2: String, p3: String, p4: String, p5: String| {
            format!("{}{}{}{}{}", &p1, &p2, &p3, &p4, &p5)
        };
        let six =
            |_: &(), p1: String, p2: String, p3: String, p4: String, p5: String, p6: String| {
                format!("{}{}{}{}{}{}", &p1, &p2, &p3, &p4, &p5, &p6)
            };
        let seven =
            |_: &(),
             p1: String,
             p2: String,
             p3: String,
             p4: String,
             p5: String,
             p6: String,
             p7: String| format!("{}{}{}{}{}{}{}", &p1, &p2, &p3, &p4, &p5, &p6, &p7);
        let unreachable = |_: &()| unreachable!();
        let router = router!(
            GET /users => zero,
            GET /users/{p1: String} => one,
            GET /users/{p1: String}/users2/{p2: String} => two,
            GET /users/{p1: String}/users2/{p2: String}/users3/{p3: String} => three,
            GET /users/{p1: String}/users2/{p2: String}/users3/{p3: String}/users4/{p4: String} => four,
            GET /users/{p1: String}/users2/{p2: String}/users3/{p3: String}/users4/{p4: String}/users5/{p5: String} => five,
            GET /users/{p1: String}/users2/{p2: String}/users3/{p3: String}/users4/{p4: String}/users5/{p5: String}/users6/{p6: String} => six,
            GET /users/{p1: String}/users2/{p2: String}/users3/{p3: String}/users4/{p4: String}/users5/{p5: String}/users6/{p6: String}/users7/{p7: String} => seven,
            _ => unreachable,
        );

        assert_eq!(router((), Method::GET, "/users"), "");
        assert_eq!(router((), Method::GET, "/users/id1"), "id1");
        assert_eq!(router((), Method::GET, "/users/id1/users2/id2"), "id1id2");
        assert_eq!(
            router((), Method::GET, "/users/id1/users2/id2/users3/id3"),
            "id1id2id3"
        );
        assert_eq!(
            router(
                (),
                Method::GET,
                "/users/id1/users2/id2/users3/id3/users4/id4"
            ),
            "id1id2id3id4"
        );
        assert_eq!(
            router(
                (),
                Method::GET,
                "/users/id1/users2/id2/users3/id3/users4/id4/users5/id5"
            ),
            "id1id2id3id4id5"
        );
        assert_eq!(
            router(
                (),
                Method::GET,
                "/users/id1/users2/id2/users3/id3/users4/id4/users5/id5/users6/id6"
            ),
            "id1id2id3id4id5id6"
        );
        assert_eq!(
            router(
                (),
                Method::GET,
                "/users/id1/users2/id2/users3/id3/users4/id4/users5/id5/users6/id6/users7/id7"
            ),
            "id1id2id3id4id5id6id7"
        );
    }
}

// cargo +nightly rustc -- -Zunstable-options --pretty=expanded
