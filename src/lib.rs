//! Valor

use http_types::Body;
pub use http_types::{Method, Request, Response, StatusCode, Url};
use registry::PluginRegistry;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
pub use vlugin::vlugin;

mod registry;

type Result = std::result::Result<Response, Response>;

/// Handler is the main entry point for dispatching incoming requests
/// to registered plugins under a specific URL pattern.
///
/// ```
/// # use http_types::{StatusCode, Request};
/// let handler = Handler::new();
/// let request = Request::new();
/// let res = handler.handle_request(request).await?;
/// assert_eq(res, StatusCode::Ok);
/// ```
pub struct Handler(Arc<PluginRegistry>);

impl Handler {
    /// Creates a new `Handler` instance
    pub fn new(loader: Arc<impl Loader>) -> Self {
        let registry = PluginRegistry::new();
        let (plugin, handler) = registry.clone().as_handler(loader);
        registry.register(plugin, handler);
        Handler(registry)
    }

    /// Handle the incoming request and send back a response
    /// from the matched plugin to the caller.
    pub async fn handle_request(&self, request: impl Into<Request>) -> Result {
        let request = request.into();
        let req_id = request
            .header("x-request-id")
            .ok_or_else(|| res(StatusCode::BadRequest, "Missing request ID"))?
            .as_str()
            .to_owned();

        let (plugin, handler) = self
            .0
            .match_plugin_handler(request.url().path())
            .ok_or_else(|| res(StatusCode::NotFound, ""))?;

        let mut response = handler.handle_request(request).await;
        response.insert_header("x-correlation-id", req_id);
        response.insert_header("x-valor-plugin", plugin.name());

        Ok(response)
    }
}

impl Clone for Handler {
    fn clone(&self) -> Self {
        Handler(self.0.clone())
    }
}

impl fmt::Debug for Handler
where
    for<'a> dyn RequestHandler + 'a: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Handler").field(&self.0).finish()
    }
}

/// Loader
pub trait Loader: Send + Sync + 'static {
    /// Loads the given `plugin`
    fn load(&self, plugin: &Plugin) -> std::result::Result<Box<dyn RequestHandler>, ()>;
}

#[inline]
pub(crate) fn res(status: StatusCode, msg: impl Into<Body>) -> Response {
    let mut res = Response::new(status);
    res.set_body(msg);
    res
}

/// Handler response
pub type HandlerResponse = Pin<Box<dyn Future<Output = Response> + Send>>;

/// Request handler
pub trait RequestHandler: Send + Sync {
    /// Handles the request
    fn handle_request(&self, request: Request) -> HandlerResponse;
}

impl<F> RequestHandler for F
where
    F: Fn(Request) -> HandlerResponse + Send + Sync,
{
    fn handle_request(&self, request: Request) -> HandlerResponse {
        self(request)
    }
}

/// Plugin
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Plugin {
    /// Built in
    BuiltIn {
        /// Name
        name: String,
    },
    /// Dummy
    Dummy,
    /// Native
    Native {
        /// Name
        name: String,
        /// Path
        path: Option<String>,
    },
    /// Web worker
    WebWorker {
        /// Name
        name: String,
        /// Url
        url: Url,
    },
}

impl Plugin {
    fn name(&self) -> String {
        match self {
            Self::Dummy => "dummy",
            Self::BuiltIn { name } => name,
            Self::Native { name, .. } => name,
            Self::WebWorker { name, .. } => name,
        }
        .into()
    }

    fn prefix(&self) -> String {
        match self {
            Self::BuiltIn { name } => ["_", name].join(""),
            Self::Dummy => "__dummy__".into(),
            Self::Native { name, .. } => name.into(),
            Self::WebWorker { name, .. } => name.into(),
        }
    }
}
