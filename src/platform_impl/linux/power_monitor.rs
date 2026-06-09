use crate::monitor::{PowerEventChannel, PowerState};
use std::thread;
use tokio::task::JoinHandle;
use zbus::{proxy, Result};

#[proxy(
  default_service = "org.freedesktop.login1",
  default_path = "/org/freedesktop/login1",
  interface = "org.freedesktop.login1.Manager"
)]
trait Manager {
  /// PrepareForShutdown signal
  #[zbus(signal)]
  fn prepare_for_shutdown(&self, start: bool) -> zbus::Result<()>;
  /// PrepareForSleep signal
  #[zbus(signal)]
  fn prepare_for_sleep(&self, start: bool) -> zbus::Result<()>;
  /// GetSession method
  fn get_session(&self, session_id: &str) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;
}

#[proxy(
  interface = "org.freedesktop.login1.Session",
  default_service = "org.freedesktop.login1",
  default_path = "/org/freedesktop/login1/session/auto"
)]
trait Session {
  #[zbus(signal)]
  fn unlock(&self) -> zbus::Result<()>;

  #[zbus(property)]
  fn locked_hint(&self) -> Result<bool>;
  #[zbus(property)]
  fn id(&self) -> zbus::Result<String>;
}

#[allow(dead_code)]
pub struct PowerMonitor {}

impl PowerMonitor {
  pub fn new() -> Self {
    Self {}
  }
}

impl PowerMonitor {
  pub fn start_listening(&self) -> std::result::Result<(), crate::monitor::Error> {
    let system_bus = zbus::blocking::Connection::system()?;
    let manager_proxy = ManagerProxyBlocking::new(&system_bus)?;
    let (mut prepare_for_shutdown, mut prepare_for_sleep) = get_suspend_monitor(&manager_proxy)?;
    let session_proxy = SessionProxyBlocking::new(&system_bus)?;
    let session_id = session_proxy.id()?;
    let session_obj_path = manager_proxy.get_session(&session_id)?;
    let login_session_proxy = SessionProxyBlocking::builder(&system_bus)
      .path(session_obj_path)?
      .build()?;
    let mut unlock = login_session_proxy.receive_unlock()?;
    let mut locked_hint = login_session_proxy.receive_locked_hint_changed();
    let runtime = tokio::runtime::Runtime::new()?;

    thread::spawn(move || {
      runtime.block_on(async {
        let mut handles: Vec<JoinHandle<()>> = vec![];
        handles.push(tokio::spawn(async move {
          while let Some(signal) = prepare_for_shutdown.next() {
            if let Ok(args) = signal.args() {
              if *args.start() {
                let sender = PowerEventChannel::sender();
                let _ = sender.send(PowerState::Shutdown);
              }
            }
          }
        }));
        handles.push(tokio::spawn(async move {
          while let Some(signal) = prepare_for_sleep.next() {
            let Ok(args) = signal.args() else {
              continue;
            };
            let sender = PowerEventChannel::sender();
            let event = if *args.start() {
              PowerState::Suspend
            } else {
              PowerState::Resume
            };
            let _ = sender.send(event);
          }
        }));
        handles.push(tokio::spawn(async move {
          while let Some(v) = locked_hint.next() {
            let Ok(status) = v.get() else {
              continue;
            };
            if status {
              let sender = PowerEventChannel::sender();
              let _ = sender.send(PowerState::ScreenLocked);
            }
          }
        }));
        handles.push(tokio::spawn(async move {
          while unlock.next().is_some() {
            let sender = PowerEventChannel::sender();
            let _ = sender.send(PowerState::ScreenUnlocked);
          }
        }));

        for handle in handles {
          let _ = handle.await;
        }
      });
    });

    Ok(())
  }
}

fn get_suspend_monitor(
  manager_proxy: &ManagerProxyBlocking,
) -> Result<(PrepareForShutdownIterator, PrepareForSleepIterator)> {
  let prepare_for_shutdown = manager_proxy.receive_prepare_for_shutdown()?;
  let prepare_for_sleep = manager_proxy.receive_prepare_for_sleep()?;
  Ok((prepare_for_shutdown, prepare_for_sleep))
}
