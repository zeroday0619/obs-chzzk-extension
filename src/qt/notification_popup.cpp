#include <obs_chzzk_extension/src/qt-rs/notification_popup.cxx.h>

#include "notification_popup.h"

#include <QApplication>
#include <QMessageBox>
#include <QMetaObject>
#include <QObject>
#include <QString>
#include <QWidget>

void show_notification_popup(::std::int32_t level, const QString &title, const QString &message)
{
    (void)level;

    QApplication *application = qobject_cast<QApplication *>(QApplication::instance());
    if (application == nullptr) {
        return;
    }

    const QString trimmed_title = title.trimmed().isEmpty()
        ? QStringLiteral("obs-chzzk-extension")
        : title.trimmed();
    const QString trimmed_message = message.trimmed();
    if (trimmed_message.isEmpty()) {
        return;
    }

    const QMessageBox::Icon icon = QMessageBox::Critical;

    QMetaObject::invokeMethod(
        application,
        [trimmed_title, trimmed_message, icon]() {
            QWidget *parent = QApplication::activeWindow();
            auto *message_box = new QMessageBox(icon, trimmed_title, trimmed_message, QMessageBox::Ok, parent);
            message_box->setAttribute(Qt::WA_DeleteOnClose);
            message_box->open();
        },
        Qt::QueuedConnection);
}
