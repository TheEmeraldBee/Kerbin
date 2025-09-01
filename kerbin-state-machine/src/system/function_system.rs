use std::marker::PhantomData;

use crate::system::param::{SystemParam, SystemParamDesc};

use super::System;
use super::into_system::IntoSystem;

pub struct FunctionSystem<Input, F> {
    f: F,
    marker: PhantomData<fn() -> Input>,
}

macro_rules! impl_for_func {
    ($($item:ident)*) => {
        impl<Fut, Func, $($item),*> System for FunctionSystem<($($item,)*), Func>
        where
            Fut: Future<Output = ()> + Send + Sync + 'static,
                for<'a, 'b> &'a Func:
                    Fn( $($item),* ) -> Fut +
                    Fn( $(<$item as SystemParam>::Item<'b>),* ) -> Fut,
            $($item: SystemParam),*
        {
            #[inline]
            #[allow(non_snake_case, unused_variables)]
            fn call<'a>(&'a self, storage: &crate::storage::StateStorage) -> futures::future::BoxFuture<'a, ()> {
                #[allow(clippy::too_many_arguments)]
                fn call_inner<'a, Fut: Future<Output = ()> + Send + Sync + 'static, $($item),*>(
                    f: impl Fn($($item),*) -> Fut,
                    $($item: $item,)*
                ) -> futures::future::BoxFuture<'a, ()> {
                    Box::pin(f($($item),*))
                }

                $(
                    let $item = $item::retrieve(storage);
                )*

                call_inner(&self.f, $($item),*)
            }

            #[inline]
            #[allow(unused_mut)]
            fn params(&self) -> Vec<SystemParamDesc> {
                vec![
                    $($item::desc()),*
                ]

            }
        }

        impl<Fut, Func, $($item),*> IntoSystem<($($item,)*), ()> for Func
        where
            Fut: Future<Output = ()> + Send + Sync + 'static,
                for<'a, 'b> &'a Func:
                    Fn( $($item),* ) -> Fut +
                    Fn( $(<$item as SystemParam>::Item<'b>),* ) -> Fut,
            $($item: SystemParam),*
        {
            type System = FunctionSystem<($($item,)*), Self>;
            fn into_system(self) -> Self::System {
                FunctionSystem {
                    f: self,
                    marker: Default::default(),
                }
            }
        }

    };
}

impl_for_func! {}
impl_for_func! { P0 }
impl_for_func! { P0 P1 }
impl_for_func! { P0 P1 P2 }
impl_for_func! { P0 P1 P2 P3 }
impl_for_func! { P0 P1 P2 P3 P4 }
impl_for_func! { P0 P1 P2 P3 P4 P5 }
impl_for_func! { P0 P1 P2 P3 P4 P5 P6 }
impl_for_func! { P0 P1 P2 P3 P4 P5 P6 P7 }
impl_for_func! { P0 P1 P2 P3 P4 P5 P6 P7 P8 }
impl_for_func! { P0 P1 P2 P3 P4 P5 P6 P7 P8 P9 }
impl_for_func! { P0 P1 P2 P3 P4 P5 P6 P7 P8 P9 P10 }
impl_for_func! { P0 P1 P2 P3 P4 P5 P6 P7 P8 P9 P10 P11 }

#[cfg(test)]
mod tests {
    use crate::{
        State,
        system::param::{res::Res, res_mut::ResMut},
    };

    async fn test_a() {}
    async fn test_b(_res: Res<bool>, _res_mut: ResMut<String>) {}
    async fn test_c(_res: Res<bool>) {}

    async fn test_a_fail(_test: Res<bool>, _fail: ResMut<bool>) {}

    #[test]
    fn test_into_state() {
        let mut state = State::new();

        state.on_hook::<()>().system(test_a);
        state.on_hook::<()>().system(test_b);
        state.on_hook::<()>().system(test_c);
    }

    #[test]
    #[should_panic]
    fn test_bad_system() {
        let mut state = State::new();

        state.on_hook::<()>().system(test_a_fail);
    }
}
