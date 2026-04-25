# Changelog

All notable user-facing changes to `tokio-fsm` will be documented in this file.

## Unreleased

### Fixed
- Improved proc-macro diagnostics: internal invariant failures now surface as structured compile errors instead of macro panics.
- Tightened handler validation: handlers must be `async fn`, and combining `#[on(...)]` with `#[on_timeout]` is rejected with a clear compile-time error.
- Corrected timeout and transition behavior: timeout-triggered transitions rearm correctly, and `Result<Transition<S>, E>` / transition-on-error forms continue to propagate as intended.
- Preserved shutdown semantics: `handle.shutdown()` no longer cancels the caller's `CancellationToken`, while parent-token cancellation still propagates and long-running handlers stop promptly.
- Improved generated runtime behavior: crate-local `tokio` re-exports avoid path hygiene issues and traced FSM execution no longer keeps span guards across `.await`.
- Hardened Axum example behavior: duplicate order IDs now return `409 Conflict`, and stop/shutdown no longer holds shared mutexes across task await points.

### Changed
- Expanded test coverage with deterministic timing paths and additional compile-fail UI checks for macro diagnostics.
- Optimized macro reachability validation to a single DFS traversal from the initial state.
- Continued macro/runtime codegen modularization with clearer contributor documentation around parse/validate/generate flow.
