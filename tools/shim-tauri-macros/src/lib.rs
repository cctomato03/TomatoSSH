extern crate proc_macro;

use proc_macro::TokenStream;

/// `#[tauri::command]` 的空实现：原样返回被标注的函数
#[proc_macro_attribute]
pub fn command(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}
