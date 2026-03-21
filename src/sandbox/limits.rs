use std::process::Command;

/// Wrap the final sandbox command with systemd-run --scope --user to apply resource limits.
pub fn wrap_with_resource_limits(
    cmd: Command,
    mem: Option<String>,
    cpu: Option<f32>,
    pid_limit: Option<u32>,
    timeout: Option<u64>,
) -> Command {
    let original_args = cmd.get_args().map(|a| a.to_os_string()).collect::<Vec<_>>();
    let program = cmd.get_program().to_os_string();
    
    let mut systemd = Command::new("systemd-run");
    systemd.arg("--scope").arg("--user");
    
    if let Some(m) = mem {
        systemd.arg("-p").arg(format!("MemoryMax={}", m));
    }
    
    if let Some(c) = cpu {
        // CPUQuota is percentage of a single core. 1.0 = 100%.
        let quota = (c * 100.0) as u32;
        systemd.arg("-p").arg(format!("CPUQuota={}%", quota));
    }
    
    if let Some(p) = pid_limit {
        systemd.arg("-p").arg(format!("TasksMax={}", p));
    }
    
    if let Some(t) = timeout {
        systemd.arg("-p").arg(format!("RuntimeMaxSec={}", t));
    }
    
    // Silence systemd-run's "Running scope as unit..." message
    systemd.arg("--quiet");
    
    systemd.arg("--").arg(program).args(&original_args);
    
    // Copy environments
    for (k, v) in cmd.get_envs() {
        if let Some(v) = v {
            systemd.env(k, v);
        } else {
            systemd.env_remove(k);
        }
    }
    
    systemd
}
