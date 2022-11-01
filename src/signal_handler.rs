use anyhow::Result;

#[cfg(unix)]
pub async fn handle_signals() -> Result<()> {
    use tokio::signal::unix::{signal, SignalKind};

    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sigterm = signal(SignalKind::terminate())?;

    tokio::select! {
        _ = sigint.recv() => log::debug!("Received SIGINT signal"),
        _ = sigterm.recv() => log::debug!("Received SIGTERM signal"),
    };
    Ok(())
}

#[cfg(windows)]
pub async fn handle_signals() -> Result<()> {
    use tokio::signal::{
        ctrl_c,
        windows::{ctrl_break, ctrl_close, ctrl_logoff, ctrl_shutdown},
    };

    let mut ctrlbreak = ctrl_break()?;
    let mut ctrlclose = ctrl_close()?;
    let mut ctrllogoff = ctrl_logoff()?;
    let mut ctrlshutdown = ctrl_shutdown()?;

    tokio::select! {
        _ = ctrl_c() => log::debug!("Received ctrl-c signal"),
        _ = ctrlbreak.recv() => log::debug!("Received ctrl-break signal"),
        _ = ctrlclose.recv() => log::debug!("Received ctrl-close signal"),
        _ = ctrllogoff.recv() => log::debug!("Received ctrl-logoff signal"),
        _ = ctrlshutdown.recv() => log::debug!("Received ctrl-shutdown signal"),
    }
    Ok(())
}
