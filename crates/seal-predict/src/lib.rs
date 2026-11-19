//! Prediction and speculation for Seal.
//!
//! Level 1 is exact: [`patterns`] reads a kernel and proves that a column
//! follows a closed form (a step, an affine map, a reset timer, or nothing
//! writes it). A proven timer becomes a [`wheel::TimingWheel`] entry instead
//! of per-tick work, and gives the same events in the same order.
//!
//! Level 2 is learned and always verified: [`stride`] keeps a stride
//! predictor with a saturating confidence counter per field.
//!
//! [`cost`] holds the cost model that decides when speculation pays,
//! [`spec`] runs speculative work on spare capacity at the lowest thread
//! priority and never makes real work wait, and [`optimistic`] runs ordered
//! batches in parallel and re-executes conflicts so the result always equals
//! serial execution.

pub mod cost;
pub mod optimistic;
pub mod patterns;
pub mod spec;
pub mod stride;
pub mod wheel;

pub use cost::{breakeven, expected_gain, predicted_reexecution, spare_budget_ms};
pub use optimistic::{execute_batch, Access, BatchStats};
pub use patterns::{analyze, Exactness, Pattern, Term, TimerBank};
pub use spec::{CancelToken, SourceStats, Speculator};
pub use stride::{FieldPredictors, Stride};
pub use wheel::TimingWheel;
