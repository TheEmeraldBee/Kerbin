use std::{
    any::{Any, TypeId},
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::Arc,
};

use crate::*;

/// The future returned by an interceptor.
pub type InterceptorFuture<'a> = Pin<Box<dyn Future<Output = InterceptorResult> + Send + 'a>>;

/// What an interceptor tells the dispatch loop to do with the intercepted command.
pub enum InterceptorResult {
    /// Run the command normally.
    Allow,
    /// Drop the command entirely.
    Cancel,
    /// Run the original command, then dispatch each follow-up command in order.
    After(Vec<Box<dyn Command>>),
    /// Skip the original command, dispatch these commands instead.
    Replace(Vec<Box<dyn Command>>),
}

#[async_trait::async_trait]
trait ErasedInterceptor: Send + Sync {
    async fn call(&self, cmd: &(dyn Any + Send + Sync), state: &mut State) -> InterceptorResult;
}

struct TypedInterceptorFn<C> {
    f: Box<dyn for<'a> Fn(&'a C, &'a mut State) -> InterceptorFuture<'a> + Send + Sync>,
}

#[async_trait::async_trait]
impl<C: Any + Send + Sync + 'static> ErasedInterceptor for TypedInterceptorFn<C> {
    async fn call(&self, cmd: &(dyn Any + Send + Sync), state: &mut State) -> InterceptorResult {
        let typed = cmd
            .downcast_ref::<C>()
            .expect("CommandInterceptorRegistry: TypeId matched but downcast failed");
        (self.f)(typed, state).await
    }
}

/// Registry for pre-dispatch command interceptors, keyed by concrete command type.
#[derive(State)]
pub struct CommandInterceptorRegistry {
    interceptors: HashMap<TypeId, Vec<Arc<dyn ErasedInterceptor>>>,
}

impl CommandInterceptorRegistry {
    pub fn new() -> Self {
        Self {
            interceptors: HashMap::new(),
        }
    }

    /// Register an interceptor for commands of type `C`.
    pub fn on_command<C>(
        &mut self,
        f: impl for<'a> Fn(&'a C, &'a mut State) -> InterceptorFuture<'a> + Send + Sync + 'static,
    ) where
        C: Command + Any + Send + Sync + 'static,
    {
        self.interceptors
            .entry(TypeId::of::<C>())
            .or_default()
            .push(Arc::new(TypedInterceptorFn { f: Box::new(f) }));
    }

    fn get_for(&self, type_id: TypeId) -> Option<Vec<Arc<dyn ErasedInterceptor>>> {
        self.interceptors.get(&type_id).cloned()
    }
}

/// Dispatch a command through the interceptor registry, then apply it.
pub async fn dispatch_command(cmd: Box<dyn Command>, state: &mut State) {
    let type_id = cmd.as_any().type_id();

    // Clone Arc refs out and drop the registry lock before touching state.
    let interceptors: Vec<Arc<dyn ErasedInterceptor>> = {
        let registry = state.lock_state::<CommandInterceptorRegistry>().await;
        registry.get_for(type_id).unwrap_or_default()
    };

    if interceptors.is_empty() {
        cmd.apply(state).await;
        return;
    }

    // Run interceptors in order; first non-Allow result wins.
    let mut result = InterceptorResult::Allow;
    for interceptor in &interceptors {
        result = interceptor.call(cmd.as_any(), state).await;
        if !matches!(result, InterceptorResult::Allow) {
            break;
        }
    }

    match result {
        InterceptorResult::Allow => {
            cmd.apply(state).await;
        }
        InterceptorResult::Cancel => {}
        InterceptorResult::After(follow_ups) => {
            cmd.apply(state).await;
            for follow_up in follow_ups {
                Box::pin(dispatch_command(follow_up, state)).await;
            }
        }
        InterceptorResult::Replace(replacements) => {
            for replacement in replacements {
                Box::pin(dispatch_command(replacement, state)).await;
            }
        }
    }
}
