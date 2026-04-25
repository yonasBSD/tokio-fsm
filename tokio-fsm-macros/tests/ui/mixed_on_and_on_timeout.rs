use tokio_fsm::fsm;

#[fsm(initial = Idle)]
impl MixedAttrFsm {
    type Context = ();
    type Error = std::convert::Infallible;

    #[on(state = Idle, event = Tick)]
    #[on_timeout]
    async fn mixed_handler(&mut self) -> tokio_fsm::Transition<Idle> {
        tokio_fsm::Transition::to(Idle)
    }
}

fn main() {}
