use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use once_cell::sync::Lazy;
use snowflake::SnowflakeIdGenerator;

static SNOWFLAKE: Lazy<Arc<Mutex<SnowflakeIdGenerator>>> = Lazy::new(|| {
    Arc::new(Mutex::new(SnowflakeIdGenerator::with_epoch(
        0,
        0,
        SystemTime::now(),
    )))
});

pub fn snowflake_new_id() -> i64 {
    SNOWFLAKE
        .lock()
        .expect("unable to lock Snowflake ID Generator")
        .generate()
}
