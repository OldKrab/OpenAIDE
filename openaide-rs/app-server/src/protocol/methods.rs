pub const RUNTIME_HEALTH: &str = "runtime.health";
pub const RUNTIME_SHUTDOWN: &str = "runtime.shutdown";

pub fn shell_local_methods() -> Vec<&'static str> {
    vec![RUNTIME_HEALTH, RUNTIME_SHUTDOWN]
}
