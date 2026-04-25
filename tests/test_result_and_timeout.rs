use std::time::Duration;

mod error_propagation {
    use tokio_fsm::{TaskError, Transition, fsm};

    #[derive(Debug, thiserror::Error)]
    pub enum HandlerError {
        #[error("boom")]
        Boom,
    }

    #[fsm(initial = Idle)]
    impl ErrorPropagationFsm {
        type Context = ();
        type Error = HandlerError;

        #[on(state = Idle, event = Start)]
        async fn on_start(&mut self, fail: bool) -> Result<Transition<Running>, HandlerError> {
            if fail {
                return Err(HandlerError::Boom);
            }

            Ok(Transition::to(Running))
        }
    }

    #[tokio::test]
    async fn test_fsm_handler_error_propagates_to_task() {
        let (handle, task) = ErrorPropagationFsm::spawn(());

        handle
            .send(ErrorPropagationFsmEvent::Start(true))
            .await
            .unwrap();

        match task.await {
            Err(TaskError::Fsm(HandlerError::Boom)) => {}
            other => panic!("expected TaskError::Fsm(HandlerError::Boom), got {other:?}"),
        }
    }
}

mod transition_result {
    use std::convert::Infallible;

    use tokio_fsm::{Transition, fsm};

    #[fsm(initial = Idle)]
    impl TransitionResultFsm {
        type Context = ();
        type Error = Infallible;

        #[on(state = Idle, event = Start)]
        async fn on_start(
            &mut self,
            fail: bool,
        ) -> Result<Transition<Running>, Transition<Failed>> {
            if fail {
                return Err(Transition::to(Failed));
            }

            Ok(Transition::to(Running))
        }
    }

    #[tokio::test]
    async fn test_result_transition_err_still_transitions_state() {
        let (handle, task) = TransitionResultFsm::spawn(());

        handle
            .send(TransitionResultFsmEvent::Start(true))
            .await
            .unwrap();
        handle
            .wait_for_state(TransitionResultFsmState::Failed)
            .await
            .unwrap();

        handle.shutdown();
        task.await.unwrap();
    }
}

#[allow(dead_code)]
mod timeout_chain {
    use std::convert::Infallible;

    use tokio_fsm::{Transition, fsm};

    #[derive(Debug, Default)]
    pub struct TimeoutChainContext {
        pub timeout_count: usize,
    }

    #[fsm(initial = Idle)]
    impl TimeoutChainFsm {
        type Context = TimeoutChainContext;
        type Error = Infallible;

        #[on(state = Idle, event = Start)]
        #[state_timeout(duration = "100ms")]
        async fn on_start(&mut self) -> Transition<Pending> {
            Transition::to(Pending)
        }

        #[on(state = Idle, event = PrimeCooling)]
        #[state_timeout(duration = "50ms")]
        async fn prime_cooling_timeout(&mut self) -> Transition<Cooling> {
            Transition::to(Cooling)
        }

        #[on_timeout]
        async fn on_timeout(&mut self) -> Result<Transition<Failed>, Transition<Cooling>> {
            self.context.timeout_count += 1;

            match self.state {
                TimeoutChainFsmState::Pending => Err(Transition::to(Cooling)),
                TimeoutChainFsmState::Cooling => Ok(Transition::to(Failed)),
                _ => Ok(Transition::to(Failed)),
            }
        }
    }
}

#[tokio::test(start_paused = true)]
async fn test_timeout_rearms_after_timeout_transition() {
    let (handle, task) =
        timeout_chain::TimeoutChainFsm::spawn(timeout_chain::TimeoutChainContext::default());

    handle
        .send(timeout_chain::TimeoutChainFsmEvent::Start)
        .await
        .unwrap();
    handle
        .wait_for_state(timeout_chain::TimeoutChainFsmState::Pending)
        .await
        .unwrap();

    tokio::time::advance(Duration::from_millis(100)).await;
    handle
        .wait_for_state(timeout_chain::TimeoutChainFsmState::Cooling)
        .await
        .unwrap();

    tokio::time::advance(Duration::from_millis(50)).await;
    handle
        .wait_for_state(timeout_chain::TimeoutChainFsmState::Failed)
        .await
        .unwrap();

    handle.shutdown();
    let context = task.await.unwrap();
    assert_eq!(context.timeout_count, 2);
}
