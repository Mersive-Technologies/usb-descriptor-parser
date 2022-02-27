use std::sync::Once;

static INIT: Once = Once::new();

// TODO: makes tests pass for now, maybe find better way later
pub fn setup_logger() {
    INIT.call_once(|| {
        pretty_env_logger::init_timed();
    });
}
