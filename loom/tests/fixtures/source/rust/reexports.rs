pub use crate::external::Thing as PublicThing;
use crate::external::make as make_value;

pub fn build() {
    let _ = make_value();
}
