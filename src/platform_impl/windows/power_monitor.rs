use crate::monitor::{PowerEventChannel, PowerState};
use windows::{
  core::s,
  Win32::{
    Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM},
    System::{
      LibraryLoader::GetModuleHandleA,
      Power::RegisterPowerSettingNotification,
      RemoteDesktop::{WTSRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION},
      SystemServices::GUID_POWERSCHEME_PERSONALITY,
    },
    UI::WindowsAndMessaging::{
      CreateWindowExA, DefWindowProcA, IsWindow, RegisterClassA, CW_USEDEFAULT,
      PBT_APMRESUMESUSPEND, PBT_APMSUSPEND, REGISTER_NOTIFICATION_FLAGS, WINDOW_EX_STYLE,
      WM_POWERBROADCAST, WM_WTSSESSION_CHANGE, WNDCLASSA, WS_OVERLAPPEDWINDOW, WTS_SESSION_LOCK,
      WTS_SESSION_UNLOCK,
    },
  },
};

#[allow(dead_code)]
pub struct PowerMonitor {
  hwnd: HWND,
}

impl PowerMonitor {
  pub fn new() -> Self {
    unsafe {
      let hwnd = create_power_events_listener().unwrap_or_default();
      Self { hwnd }
    }
  }

  pub fn start_listening(&self) -> std::result::Result<(), crate::monitor::Error> {
    unsafe {
      if !IsWindow(Some(self.hwnd)).as_bool() {
        return Err("Failed to create power events listener window".into());
      }

      if RegisterPowerSettingNotification(
        HANDLE(self.hwnd.0),
        &GUID_POWERSCHEME_PERSONALITY,
        REGISTER_NOTIFICATION_FLAGS(0),
      )
      .is_err()
      {
        return Err("Failed to register power setting notification".into());
      };
      if WTSRegisterSessionNotification(self.hwnd, NOTIFY_FOR_THIS_SESSION).is_err() {
        return Err("Failed to register session notification".into());
      };
    }
    Ok(())
  }
}

impl Default for PowerMonitor {
  fn default() -> Self {
    Self::new()
  }
}

unsafe fn create_power_events_listener() -> std::result::Result<HWND, crate::monitor::Error> {
  let instance = GetModuleHandleA(None)?;

  let window_class = s!("__psp_event_listener");

  let wnd_class = WNDCLASSA {
    hInstance: instance.into(),
    lpszClassName: window_class,
    lpfnWndProc: Some(wndproc),
    ..Default::default()
  };

  RegisterClassA(&wnd_class);

  let hwnd = CreateWindowExA(
    WINDOW_EX_STYLE::default(),
    window_class,
    s!("__psp_dummy_window"),
    WS_OVERLAPPEDWINDOW,
    CW_USEDEFAULT,
    CW_USEDEFAULT,
    CW_USEDEFAULT,
    CW_USEDEFAULT,
    None,
    None,
    Some(instance.into()),
    None,
  );

  let hwnd = hwnd?;

  if !IsWindow(Some(hwnd)).as_bool() {
    return Err("Unable to get valid mutable pointer for CreateWindowEx".into());
  }

  Ok(hwnd)
}

extern "system" fn wndproc(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
  unsafe {
    match message {
      WM_POWERBROADCAST => match wparam.0 as u32 {
        PBT_APMRESUMESUSPEND => {
          let sender = PowerEventChannel::sender();
          let _ = sender.send(PowerState::Resume);
        }
        PBT_APMSUSPEND => {
          let sender = PowerEventChannel::sender();
          let _ = sender.send(PowerState::Suspend);
        }
        _ => {}
      },
      WM_WTSSESSION_CHANGE => match wparam.0 as u32 {
        WTS_SESSION_LOCK => {
          let sender = PowerEventChannel::sender();
          let _ = sender.send(PowerState::ScreenLocked);
        }
        WTS_SESSION_UNLOCK => {
          let sender = PowerEventChannel::sender();
          let _ = sender.send(PowerState::ScreenUnlocked);
        }
        _ => {}
      },
      _ => {}
    }
    DefWindowProcA(window, message, wparam, lparam)
  }
}
