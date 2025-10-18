use crate::errors::{RateLimitError, Result};
use redis::{AsyncCommands, Script};
use tracing::{info, debug};

/// Load and register the Lua script with Redis
pub async fn load_script<C: AsyncCommands>(conn: &mut C) -> Result<String> {
    let script_content = include_str!("../../scripts/leaky_bucket.lua");
    
    debug!("Loading Lua script into Redis...");
    
    let script = Script::new(script_content);
    let sha = script.prepare_invoke()
        .load_async(conn)
        .await
        .map_err(|e| RateLimitError::ScriptExecutionError(
            format!("Failed to load Lua script: {}", e)
        ))?;
    
    info!("Lua script loaded successfully (SHA: {})", sha);
    debug!("Lua script loaded successfully with (SHA: {})", sha);
    Ok(sha)
}

/// Get the script object for execution
pub fn get_script() -> Script {
    let script_content = include_str!("../../scripts/leaky_bucket.lua");
    Script::new(script_content)
}