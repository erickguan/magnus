// Reuse the workspace library's implementation in the installed extension.
// The standalone manifest pins Magnus so a source gem needs no parent checkout.
// Rust-only wrapper methods remain available to workspace library consumers.
#[allow(dead_code)]
#[path = "../../../src/lib.rs"]
mod implementation;

#[magnus::init(name = "magnus_jiff_timezone")]
fn init(ruby: &magnus::Ruby) -> Result<(), magnus::Error> {
    implementation::register(ruby)
}
