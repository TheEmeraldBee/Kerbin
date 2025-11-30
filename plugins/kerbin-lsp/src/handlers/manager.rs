use std::pin::Pin;
use std::sync::Arc;

use std::collections::HashMap;

use crate::*;
use kerbin_core::*;

pub type EventHandler = Arc<
    dyn for<'a> Fn(&'a State, &'a JsonRpcMessage) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>
        + Send
        + Sync,
>;

pub struct HandlerEntry {
    pub hook_info: HookInfo,
    pub handler: EventHandler,
}

#[derive(Default)]
pub struct HandlerSet {
    pub response_handlers: Vec<HandlerEntry>,
    pub notification_handlers: Vec<HandlerEntry>,
    pub server_request_handlers: Vec<HandlerEntry>,
}

#[derive(Default, State)]
pub struct LspHandlerManager {
    handler_map: HashMap<String, HandlerSet>,

    global_handlers: HandlerSet,
}

impl LspHandlerManager {
    pub fn on_global_response<F>(&mut self, pattern: &str, handler: F)
    where
        F: for<'a> Fn(
                &'a State,
                &'a JsonRpcMessage,
            ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>
            + Send
            + Sync
            + 'static,
    {
        self.global_handlers.response_handlers.push(HandlerEntry {
            hook_info: HookInfo::new_custom_split(pattern, "/"),
            handler: Arc::new(handler),
        });
    }

    pub fn on_lang_response<F>(&mut self, lang: impl ToString, pattern: &str, handler: F)
    where
        F: for<'a> Fn(
                &'a State,
                &'a JsonRpcMessage,
            ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>
            + Send
            + Sync
            + 'static,
    {
        self.handler_map
            .entry(lang.to_string())
            .or_default()
            .response_handlers
            .push(HandlerEntry {
                hook_info: HookInfo::new_custom_split(pattern, "/"),
                handler: Arc::new(handler),
            });
    }

    pub fn on_global_notify<F>(&mut self, pattern: &str, handler: F)
    where
        F: for<'a> Fn(
                &'a State,
                &'a JsonRpcMessage,
            ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>
            + Send
            + Sync
            + 'static,
    {
        self.global_handlers
            .notification_handlers
            .push(HandlerEntry {
                hook_info: HookInfo::new_custom_split(pattern, "/"),
                handler: Arc::new(handler),
            });
    }

    pub fn on_lang_notify<F>(&mut self, lang: impl ToString, pattern: &str, handler: F)
    where
        F: for<'a> Fn(
                &'a State,
                &'a JsonRpcMessage,
            ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>
            + Send
            + Sync
            + 'static,
    {
        self.handler_map
            .entry(lang.to_string())
            .or_default()
            .notification_handlers
            .push(HandlerEntry {
                hook_info: HookInfo::new_custom_split(pattern, "/"),
                handler: Arc::new(handler),
            });
    }

    pub fn on_global_server_request<F>(&mut self, pattern: &str, handler: F)
    where
        F: for<'a> Fn(
                &'a State,
                &'a JsonRpcMessage,
            ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>
            + Send
            + Sync
            + 'static,
    {
        self.global_handlers
            .server_request_handlers
            .push(HandlerEntry {
                hook_info: HookInfo::new_custom_split(pattern, "/"),
                handler: Arc::new(handler),
            });
    }

    pub fn on_lang_server_request<F>(&mut self, lang: impl ToString, pattern: &str, handler: F)
    where
        F: for<'a> Fn(
                &'a State,
                &'a JsonRpcMessage,
            ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>
            + Send
            + Sync
            + 'static,
    {
        self.handler_map
            .entry(lang.to_string())
            .or_default()
            .server_request_handlers
            .push(HandlerEntry {
                hook_info: HookInfo::new_custom_split(pattern, "/"),
                handler: Arc::new(handler),
            });
    }

    pub fn iter_response_handlers<'a>(
        &'a self,
        lang: &str,
    ) -> Box<dyn Iterator<Item = &'a HandlerEntry> + Send + Sync + 'a> {
        let iter = self.global_handlers.response_handlers.iter();

        if let Some(map) = self.handler_map.get(lang) {
            Box::new(iter.chain(map.response_handlers.iter()))
        } else {
            Box::new(iter)
        }
    }

    pub fn iter_notification_handlers<'a>(
        &'a self,
        lang: &str,
    ) -> Box<dyn Iterator<Item = &'a HandlerEntry> + Send + Sync + 'a> {
        let iter = self.global_handlers.notification_handlers.iter();

        if let Some(map) = self.handler_map.get(lang) {
            Box::new(iter.chain(map.notification_handlers.iter()))
        } else {
            Box::new(iter)
        }
    }

    pub fn iter_server_request_handlers<'a>(
        &'a self,
        lang: &str,
    ) -> Box<dyn Iterator<Item = &'a HandlerEntry> + Send + Sync + 'a> {
        let iter = self.global_handlers.server_request_handlers.iter();

        if let Some(map) = self.handler_map.get(lang) {
            Box::new(iter.chain(map.server_request_handlers.iter()))
        } else {
            Box::new(iter)
        }
    }
}
