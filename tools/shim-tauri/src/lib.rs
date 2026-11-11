//! 仅用于「本机无 Tauri 构建环境」时做静态编译校验的 tauri 桩实现。
//! 只覆盖 TomatoSSH 后端真正用到的 API 子集。
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::Arc;

pub use tauri_macros_shim::command;

#[derive(Debug)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// 事件发送能力（真实 tauri 里由 AppHandle / Window 等实现）
pub trait Emitter<R: Runtime = Wry> {
    fn emit<S: serde::Serialize + Clone>(&self, event: &str, payload: S) -> Result<()>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Wry;

pub trait Runtime: Clone + Send + Sync + 'static {}
impl Runtime for Wry {}

#[derive(Clone)]
pub struct AppHandle<R: Runtime = Wry> {
    pub config_dir: Arc<PathBuf>,
    pub events: Arc<std::sync::Mutex<Vec<(String, String)>>>,
    _rt: PhantomData<R>,
}

impl<R: Runtime> AppHandle<R> {
    pub fn new() -> Self {
        Self {
            config_dir: Arc::new(std::env::temp_dir().join("tomatossh-shim")),
            events: Arc::new(std::sync::Mutex::new(Vec::new())),
            _rt: PhantomData,
        }
    }

    /// 取出并清空已记录的事件（仅用于本地冒烟测试）
    pub fn take_events(&self) -> Vec<(String, String)> {
        let mut guard = self.events.lock().unwrap();
        std::mem::take(&mut *guard)
    }
}

impl<R: Runtime> Default for AppHandle<R> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R: Runtime> Emitter<R> for AppHandle<R> {
    fn emit<S: serde::Serialize + Clone>(&self, event: &str, payload: S) -> Result<()> {
        if let Ok(json) = serde_json::to_string(&payload) {
            if let Ok(mut guard) = self.events.lock() {
                guard.push((event.to_string(), json));
            }
        }
        Ok(())
    }
}

pub struct PathResolver {
    base: PathBuf,
}

impl PathResolver {
    pub fn app_config_dir(&self) -> Result<PathBuf> {
        Ok(self.base.clone())
    }
    pub fn app_data_dir(&self) -> Result<PathBuf> {
        Ok(self.base.clone())
    }
}

pub trait Manager<R: Runtime = Wry> {
    fn path(&self) -> PathResolver;
    fn manage<T: Send + Sync + 'static>(&self, state: T) -> bool;
    fn state<T: Send + Sync + 'static>(&self) -> State<'_, T>;
    fn get_webview_window(&self, label: &str) -> Option<WebviewWindow<R>>;
}

/// 供 `Queue` 模块使用的异步运行时桩
pub mod async_runtime {
    /// 编译校验用：不真正调度任务
    pub fn spawn<F>(_future: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
    }
}

#[derive(Clone, Debug)]
pub enum WebviewUrl {
    App(std::path::PathBuf),
    External(String),
}

#[derive(Clone)]
pub struct WebviewWindow<R: Runtime = Wry> {
    pub label: String,
    _rt: PhantomData<R>,
}

impl<R: Runtime> WebviewWindow<R> {
    pub fn set_focus(&self) -> Result<()> {
        Ok(())
    }
}

impl<R: Runtime> Emitter<R> for WebviewWindow<R> {
    fn emit<S: serde::Serialize + Clone>(&self, event: &str, payload: S) -> Result<()> {
        let _ = (event, serde_json::to_string(&payload).ok());
        Ok(())
    }
}

pub struct WebviewWindowBuilder<R: Runtime = Wry> {
    label: String,
    _rt: PhantomData<R>,
}

impl<R: Runtime> WebviewWindowBuilder<R> {
    pub fn new<L: Into<String>>(_app: &AppHandle<R>, label: L, _url: WebviewUrl) -> Self {
        Self {
            label: label.into(),
            _rt: PhantomData,
        }
    }
    pub fn title(self, _title: &str) -> Self {
        self
    }
    pub fn inner_size(self, _w: f64, _h: f64) -> Self {
        self
    }
    pub fn min_inner_size(self, _w: f64, _h: f64) -> Self {
        self
    }
    pub fn build(self) -> Result<WebviewWindow<R>> {
        Ok(WebviewWindow {
            label: self.label,
            _rt: PhantomData,
        })
    }
}

pub struct State<'r, T: Send + Sync + 'static> {
    pub inner: &'r T,
}

impl<'r, T: Send + Sync + 'static> Deref for State<'r, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.inner
    }
}

impl<'r, T: Send + Sync + 'static> DerefMut for State<'r, T> {
    fn deref_mut(&mut self) -> &mut T {
        unreachable!("shim State 不支持可变解引用")
    }
}

pub struct App<R: Runtime = Wry> {
    handle: AppHandle<R>,
    states: std::sync::Mutex<Vec<Box<dyn std::any::Any + Send + Sync>>>,
}

impl<R: Runtime> Default for App<R> {
    fn default() -> Self {
        Self {
            handle: AppHandle::new(),
            states: std::sync::Mutex::new(Vec::new()),
        }
    }
}

impl<R: Runtime> Manager<R> for AppHandle<R> {
    fn path(&self) -> PathResolver {
        PathResolver {
            base: (*self.config_dir).clone(),
        }
    }
    fn manage<T: Send + Sync + 'static>(&self, _state: T) -> bool {
        true
    }
    fn state<T: Send + Sync + 'static>(&self) -> State<'_, T> {
        panic!("shim 不支持从 AppHandle 取状态")
    }
    fn get_webview_window(&self, _label: &str) -> Option<WebviewWindow<R>> {
        None
    }
}

impl<R: Runtime> Manager<R> for App<R> {
    fn path(&self) -> PathResolver {
        PathResolver {
            base: (*self.handle.config_dir).clone(),
        }
    }
    fn manage<T: Send + Sync + 'static>(&self, state: T) -> bool {
        if let Ok(mut guard) = self.states.lock() {
            guard.push(Box::new(state));
            true
        } else {
            false
        }
    }
    fn state<T: Send + Sync + 'static>(&self) -> State<'_, T> {
        panic!("shim 不支持从 App 取状态")
    }

    fn get_webview_window(&self, _label: &str) -> Option<WebviewWindow<R>> {
        None
    }
}

pub struct Builder<R: Runtime = Wry> {
    _rt: PhantomData<R>,
}

impl<R: Runtime> Default for Builder<R> {
    fn default() -> Self {
        Self { _rt: PhantomData }
    }
}

impl<R: Runtime> Builder<R> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn plugin<P>(self, _plugin: P) -> Self {
        self
    }

    pub fn setup<F>(self, _setup: F) -> Self
    where
        F: FnOnce(&mut App<R>) -> std::result::Result<(), Box<dyn std::error::Error>> + Send + 'static,
    {
        self
    }

    pub fn invoke_handler<F>(self, _handler: F) -> Self
    where
        F: Fn(Invoke<R>) -> bool + Send + Sync + 'static,
    {
        self
    }

    pub fn run(self, _context: Context<R>) -> Result<()> {
        Ok(())
    }
}

pub struct Invoke<R: Runtime = Wry> {
    _rt: PhantomData<R>,
}

pub struct Context<R: Runtime = Wry> {
    _rt: PhantomData<R>,
}

#[macro_export]
macro_rules! generate_handler {
    ($($t:tt)*) => {
        |_invoke: $crate::Invoke| -> bool { true }
    };
}

#[macro_export]
macro_rules! generate_context {
    () => {
        $crate::Context::default()
    };
}

impl<R: Runtime> Default for Context<R> {
    fn default() -> Self {
        Self { _rt: PhantomData }
    }
}
