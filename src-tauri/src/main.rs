// Не открывать консольное окно в release на Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    yeskia_app_lib::run()
}
