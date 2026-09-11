//! Sets napi-rs up for the target platform: on macOS, lets the Node-API
//! symbols resolve when Node loads the library. See `napi_build::setup`.

fn main() {
    napi_build::setup();
}
