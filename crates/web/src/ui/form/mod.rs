//! Form controls. All bind to `RwSignal`s so pages own their draft state.

mod choice;
mod field;
mod input;
mod switch;

pub use choice::{Choice, RadioCards, Segmented, Slider};
pub use field::{Field, FormSection};
pub use input::{NumberInput, TextInput};
pub use switch::Switch;
