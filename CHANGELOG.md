# Changelog

All notable user-facing changes to `tokio-fsm` will be documented in this file.

## Unreleased

### Fixed
- Handler errors now propagate as `TaskError::Fsm(E)` for handlers that return `Result<Transition<S>, E>`.
- Existing `Result<Transition<S>, Transition<T>>` handlers continue to work as transition-on-error handlers.
- Timeout-triggered transitions now rearm the next state's timeout correctly.
- Timeout reset logic is centralized instead of being open-coded across branches.
- Duplicate `#[on(state = ...)]` declarations on the same handler now produce a clear compile-time error.
- Invalid FSM handler return types now fail with a clear compile-time error.
- `#[fsm(serde = true)]` now fails with a clear compile-time message when the `tokio-fsm` `serde` feature is not enabled.
- `handle.shutdown()` no longer cancels the caller's `CancellationToken`; the FSM now owns a child token.
- Parent `CancellationToken` cancellation still propagates into the FSM.
- Long-running handlers now stop promptly when shutdown is requested.
- Generated code now uses the crate-local `tokio` re-export, which avoids path hygiene problems when consumers rename `tokio`.
- Traced FSM execution no longer holds a tracing span guard across `.await`.

### Changed
- Added focused runtime coverage for error propagation, timeout chaining, shutdown semantics, parent-token behavior, and traced FSM execution.
- Added focused UI coverage for duplicate source-state handlers, invalid handler return types, and missing `serde` feature diagnostics.
- Refactored macro runtime code generation into smaller modules and added contributor-facing notes for the parse/validate/generate flow.
