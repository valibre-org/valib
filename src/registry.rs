use crate::{res, HandlerResponse, Loader, Method, Plugin, Request, RequestHandler, StatusCode};
use path_tree::PathTree;
use serde_json as json;
use std::collections::HashMap;
use std::fmt;
use std::iter::Iterator;
use std::sync::{Arc, Mutex};

type PluginHandler = (Plugin, Arc<dyn RequestHandler>);

/// Plugin to keep track of registered plugins
pub(crate) struct PluginRegistry {
    plugins: Mutex<HashMap<String, PluginHandler>>,
    routes: Mutex<PathTree<String>>,
}

impl PluginRegistry {
    const NAME: &'static str = "plugins";

    pub fn new() -> Arc<Self> {
        Arc::new(PluginRegistry {
            plugins: Mutex::new(HashMap::new()),
            routes: Mutex::new(PathTree::new()),
        })
    }

    pub fn match_plugin_handler(&self, path: &str) -> Option<PluginHandler> {
        let routes = self.routes.lock().unwrap();
        let plugins = self.plugins.lock().unwrap();
        let (name, _) = routes.find(path)?;
        let (plugin, handler) = plugins.get(name)?;
        Some((plugin.clone(), handler.clone()))
    }

    pub fn register(&self, plugin: Plugin, handler: Box<dyn RequestHandler>) {
        let mut routes = self.routes.lock().unwrap();
        let mut plugins = self.plugins.lock().unwrap();
        routes.insert(&plugin.prefix(), plugin.name());
        plugins.insert(plugin.name(), (plugin, handler.into()));
    }

    fn plugin_list(&self) -> Vec<Plugin> {
        self.plugins
            .lock()
            .unwrap()
            .values()
            .map(|(p, _)| p.clone())
            .collect()
    }

    pub fn as_handler(
        self: Arc<Self>,
        loader: Arc<impl Loader>,
    ) -> (Plugin, Box<dyn RequestHandler>) {
        (
            Plugin::BuiltIn {
                name: Self::NAME.into(),
            },
            Box::new(move |mut req: Request| {
                let registry = self.clone();
                let loader = loader.clone();
                Box::pin(async move {
                    match req.method() {
                        Method::Get => {
                            let plugins = registry.plugin_list();
                            json::to_vec(&plugins)
                                .map_or(res(StatusCode::InternalServerError, ""), |list| {
                                    list.into()
                                })
                        }
                        Method::Post => match req.body_json().await {
                            Ok(plugin) => match loader.load(&plugin) {
                                Ok(handler) => {
                                    registry.register(plugin, handler);
                                    res(StatusCode::Created, "")
                                }
                                Err(_) => res(StatusCode::UnprocessableEntity, ""),
                            },
                            Err(_) => res(StatusCode::BadRequest, ""),
                        },
                        _ => res(StatusCode::MethodNotAllowed, ""),
                    }
                }) as HandlerResponse
            }),
        )
    }
}

impl fmt::Debug for PluginRegistry
where
    for<'a> dyn RequestHandler + 'a: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PluginRegistry")
            .field("plugins", &self.plugins)
            .field("routes", &self.routes)
            .finish()
    }
}
