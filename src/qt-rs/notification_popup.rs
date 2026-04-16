#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        include!("notification_popup.h");
        type QString = cxx_qt_lib::QString;

        #[cxx_name = "show_notification_popup"]
        fn show_notification_popup(level: i32, title: &QString, message: &QString);
    }
}
