//! Reading and editing a chart. Every rule it obeys comes from `aurum-core`.

pub mod controls;
pub mod editor;
pub mod view;

pub use controls::{ArrangementChoice, ChartControls};
pub use editor::{ChartEditor, SaveInput};
pub use view::ChartView;
