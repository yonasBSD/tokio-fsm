use proc_macro2::TokenStream;
use quote::quote;

use crate::validation::FsmStructure;

pub fn render_task_impl(fsm: &FsmStructure) -> TokenStream {
    let task_name = fsm.task_ident();
    let context_type = &fsm.context_type;
    let error_type = &fsm.error_type;

    quote! {
        impl std::future::Future for #task_name {
            type Output = Result<#context_type, tokio_fsm::TaskError<#error_type>>;

            fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
                match std::pin::Pin::new(&mut self.handle).poll(cx) {
                    std::task::Poll::Ready(Ok(Ok(res))) => std::task::Poll::Ready(Ok(res)),
                    std::task::Poll::Ready(Ok(Err(e))) => std::task::Poll::Ready(Err(tokio_fsm::TaskError::Fsm(e))),
                    std::task::Poll::Ready(Err(e)) => std::task::Poll::Ready(Err(tokio_fsm::TaskError::Join(e))),
                    std::task::Poll::Pending => std::task::Poll::Pending,
                }
            }
        }
    }
}

pub fn render_task_drop(fsm: &FsmStructure) -> TokenStream {
    let task_name = fsm.task_ident();

    let abort_log = if fsm.tracing {
        let fsm_name = fsm.fsm_name.to_string();
        quote! {
            ::tokio_fsm::tracing::warn!(fsm = #fsm_name, "FSM task dropped before completion; aborting execution. Did you forget to retain the task handle?");
        }
    } else {
        quote! {}
    };

    quote! {
        impl Drop for #task_name {
            fn drop(&mut self) {
                #abort_log
                self.handle.abort();
            }
        }
    }
}
