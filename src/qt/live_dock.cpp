#include <obs_chzzk_extension/src/qt-rs/live_dock.cxxqt.h>
#include <QComboBox>
#include <QFrame>
#include <QHBoxLayout>
#include <QLabel>
#include <QLineEdit>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QNetworkAccessManager>
#include <QPixmap>
#include <QPushButton>
#include <QResizeEvent>
#include <QSizePolicy>
#include <QString>
#include <QTimer>
#include <QUrl>
#include <QVBoxLayout>
#include <QWidget>

class LiveDockWidget final : public QWidget {
public:
    LiveDockWidget()
    {
        auto *root = new QVBoxLayout(this);
        root->setContentsMargins(8, 8, 8, 8);
        root->setSpacing(8);

        auto *title = new QLabel("CHZZK Live Editor", this);
        root->addWidget(title);

        auto *load_button = new QPushButton("Load Current", this);
        root->addWidget(load_button);

        auto *line = new QFrame(this);
        line->setFrameShape(QFrame::HLine);
        root->addWidget(line);

        root->addWidget(new QLabel("Live Title", this));
        live_title_edit_ = new QLineEdit(this);
        root->addWidget(live_title_edit_);

        root->addWidget(new QLabel("Tags (comma-separated)", this));
        tags_edit_ = new QLineEdit(this);
        root->addWidget(tags_edit_);

        root->addWidget(new QLabel("Category Search Query", this));
        category_query_edit_ = new QLineEdit(this);
        root->addWidget(category_query_edit_);

        auto *search_button = new QPushButton("Search Category", this);
        root->addWidget(search_button);

        root->addWidget(new QLabel("Category Results", this));
        category_results_combo_ = new QComboBox(this);
        category_results_combo_->setEnabled(false);
        category_results_combo_->addItem("No results");
        root->addWidget(category_results_combo_);

        root->addWidget(new QLabel("Sort", this));
        sort_mode_combo_ = new QComboBox(this);
        sort_mode_combo_->addItem("Relevance", QStringLiteral("relevance"));
        sort_mode_combo_->addItem("Name", QStringLiteral("name"));
        root->addWidget(sort_mode_combo_);

        root->addWidget(new QLabel("Selected Category Name", this));
        category_name_edit_ = new QLineEdit(this);
        category_name_edit_->setReadOnly(true);
        root->addWidget(category_name_edit_);

        root->addWidget(new QLabel("Category Type", this));
        category_type_edit_ = new QLineEdit(this);
        root->addWidget(category_type_edit_);

        root->addWidget(new QLabel("Category ID", this));
        category_id_edit_ = new QLineEdit(this);
        root->addWidget(category_id_edit_);

        root->addWidget(new QLabel("Category Thumbnail", this));
        thumbnail_preview_label_ = new QLabel("No thumbnail", this);
        thumbnail_preview_label_->setFrameShape(QFrame::StyledPanel);
        thumbnail_preview_label_->setAlignment(Qt::AlignCenter);
        thumbnail_preview_label_->setFixedHeight(150);
        thumbnail_preview_label_->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
        root->addWidget(thumbnail_preview_label_);

        auto *apply_row = new QHBoxLayout();
        auto *apply_button = new QPushButton("Apply", this);
        auto *clear_button = new QPushButton("Clear Category", this);
        auto *clear_tags_button = new QPushButton("Clear Tags", this);
        apply_row->addWidget(apply_button);
        apply_row->addWidget(clear_button);
        apply_row->addWidget(clear_tags_button);
        root->addLayout(apply_row);

        root->addWidget(new QLabel("Status", this));
        status_label_ = new QLabel("Ready", this);
        status_label_->setWordWrap(true);
        root->addWidget(status_label_);

        root->addWidget(new QLabel("Authorization", this));
        auth_status_label_ = new QLabel("Loading...", this);
        auth_status_label_->setWordWrap(true);
        root->addWidget(auth_status_label_);

        thumbnail_network_manager_ = new QNetworkAccessManager(this);
        thumbnail_resize_timer_ = new QTimer(this);
        thumbnail_resize_timer_->setSingleShot(true);
        bridge_ = new LiveDockBridge(this);
        connect(thumbnail_resize_timer_, &QTimer::timeout, this, [this]() {
            apply_scaled_thumbnail_pixmap();
        });

        connect(load_button, &QPushButton::clicked, this, [this]() {
            bridge_->loadCurrent();
            sync_view_from_bridge();
        });

        connect(search_button, &QPushButton::clicked, this, [this]() {
            bridge_->searchCategory(category_query_edit_->text().trimmed(), use_name_sort());
            sync_view_from_bridge();
        });

        connect(category_results_combo_, qOverload<int>(&QComboBox::currentIndexChanged), this, [this](int index) {
            apply_category_selection(index);
        });

        connect(sort_mode_combo_, qOverload<int>(&QComboBox::currentIndexChanged), this, [this](int) {
            bridge_->refreshCategoryResults(category_query_edit_->text().trimmed(), use_name_sort());
            sync_view_from_bridge();
        });

        connect(apply_button, &QPushButton::clicked, this, [this]() {
            bridge_->applyUpdate(
                live_title_edit_->text().trimmed(),
                category_type_edit_->text().trimmed(),
                category_id_edit_->text().trimmed(),
                tags_edit_->text().trimmed());
            sync_view_from_bridge();
        });

        connect(clear_button, &QPushButton::clicked, this, [this]() {
            bridge_->clearCategory();
            sync_view_from_bridge();
        });

        connect(clear_tags_button, &QPushButton::clicked, this, [this]() {
            bridge_->clearTags();
            sync_view_from_bridge();
        });

        bridge_->refreshAuthState();
        sync_view_from_bridge();
    }

private:
    QLineEdit *live_title_edit_ = nullptr;
    QLineEdit *tags_edit_ = nullptr;
    QLineEdit *category_query_edit_ = nullptr;
    QComboBox *category_results_combo_ = nullptr;
    QComboBox *sort_mode_combo_ = nullptr;
    QLineEdit *category_name_edit_ = nullptr;
    QLineEdit *category_type_edit_ = nullptr;
    QLineEdit *category_id_edit_ = nullptr;
    QLabel *thumbnail_preview_label_ = nullptr;
    QLabel *status_label_ = nullptr;
    QLabel *auth_status_label_ = nullptr;
    QNetworkAccessManager *thumbnail_network_manager_ = nullptr;
    QNetworkReply *thumbnail_reply_ = nullptr;
    QTimer *thumbnail_resize_timer_ = nullptr;
    LiveDockBridge *bridge_ = nullptr;
    QPixmap original_thumbnail_pixmap_;
    QString current_thumbnail_url_;

    static constexpr int ThumbnailRescaleDebounceMs = 40;

    bool use_name_sort() const
    {
        if (sort_mode_combo_ == nullptr) {
            return false;
        }

        return sort_mode_combo_->currentData().toString() == QStringLiteral("name");
    }

    void sync_auth_status_from_bridge()
    {
        if (bridge_ == nullptr || auth_status_label_ == nullptr) {
            return;
        }

        auth_status_label_->setText(bridge_->getAuth_status());
        if (bridge_->getLinked()) {
            auth_status_label_->setStyleSheet(QStringLiteral("color: #1b7f3b;"));
        } else {
            auth_status_label_->setStyleSheet(QStringLiteral("color: #a65e00;"));
        }
    }

    void sync_thumbnail_from_bridge()
    {
        if (bridge_ == nullptr) {
            return;
        }

        const QString poster_url = bridge_->getPoster_image_url().trimmed();
        const QString category_id = bridge_->getCategory_id().trimmed();
        if (!poster_url.isEmpty()) {
            load_thumbnail_preview(poster_url);
        } else if (category_id.isEmpty()) {
            clear_thumbnail_preview("No thumbnail");
        } else {
            clear_thumbnail_preview("Thumbnail unavailable");
        }
    }

    void sync_status_from_bridge()
    {
        if (bridge_ == nullptr || status_label_ == nullptr) {
            return;
        }

        const QString status = bridge_->getStatus();
        if (!status.isEmpty()) {
            status_label_->setText(status);
        }
    }

    void sync_form_fields_from_bridge()
    {
        if (bridge_ == nullptr) {
            return;
        }

        live_title_edit_->setText(bridge_->getLive_title());
        tags_edit_->setText(bridge_->getTags_text());
        category_type_edit_->setText(bridge_->getCategory_type());
        category_id_edit_->setText(bridge_->getCategory_id());
        category_name_edit_->setText(bridge_->getCategory_name());
    }

    void sync_view_from_bridge()
    {
        if (bridge_ == nullptr) {
            return;
        }

        sync_auth_status_from_bridge();
        sync_form_fields_from_bridge();
        sync_status_from_bridge();
        refresh_category_results_combo();
        sync_thumbnail_from_bridge();
    }

    void clear_thumbnail_preview(const QString &message)
    {
        if (thumbnail_preview_label_ == nullptr) {
            return;
        }

        original_thumbnail_pixmap_ = QPixmap();
        current_thumbnail_url_.clear();

        if (thumbnail_resize_timer_ != nullptr) {
            thumbnail_resize_timer_->stop();
        }

        thumbnail_preview_label_->setPixmap(QPixmap());
        thumbnail_preview_label_->setText(message);
    }

    QSize thumbnail_target_size() const
    {
        if (thumbnail_preview_label_ == nullptr) {
            return QSize(320, 150);
        }

        QSize size = thumbnail_preview_label_->size();
        if (size.width() <= 0 || size.height() <= 0) {
            return QSize(320, 150);
        }

        const int bounded_width = std::min(size.width(), 420);
        return QSize(bounded_width, size.height());
    }

    void apply_scaled_thumbnail_pixmap()
    {
        if (thumbnail_preview_label_ == nullptr || original_thumbnail_pixmap_.isNull()) {
            return;
        }

        const QSize target_size = thumbnail_target_size();
        const QPixmap scaled = original_thumbnail_pixmap_.scaled(
            target_size,
            Qt::KeepAspectRatio,
            Qt::FastTransformation);

        thumbnail_preview_label_->setText(QString());
        thumbnail_preview_label_->setPixmap(scaled);
    }

    void cancel_thumbnail_request()
    {
        if (thumbnail_reply_ == nullptr) {
            return;
        }

        disconnect(thumbnail_reply_, nullptr, this, nullptr);
        if (thumbnail_reply_->isRunning()) {
            thumbnail_reply_->abort();
        }
        thumbnail_reply_->deleteLater();
        thumbnail_reply_ = nullptr;
    }

    void load_thumbnail_preview(const QString &thumbnail_url)
    {
        cancel_thumbnail_request();

        const QString url = thumbnail_url.trimmed();
        if (url.isEmpty()) {
            clear_thumbnail_preview("No thumbnail");
            return;
        }

        const QUrl parsed_url(url);
        if (!parsed_url.isValid() || parsed_url.scheme().isEmpty()) {
            clear_thumbnail_preview("Invalid thumbnail URL");
            return;
        }

        if (url == current_thumbnail_url_ && !original_thumbnail_pixmap_.isNull()) {
            apply_scaled_thumbnail_pixmap();
            return;
        }

        clear_thumbnail_preview("Loading thumbnail...");

        QNetworkRequest request(parsed_url);

        thumbnail_reply_ = thumbnail_network_manager_->get(request);
        connect(thumbnail_reply_, &QNetworkReply::finished, this, [this]() {
            QNetworkReply *reply = thumbnail_reply_;
            thumbnail_reply_ = nullptr;

            if (reply == nullptr) {
                clear_thumbnail_preview("No thumbnail");
                return;
            }

            const bool request_ok = reply->error() == QNetworkReply::NoError;
            const QByteArray data = reply->readAll();
            reply->deleteLater();

            if (!request_ok || data.isEmpty()) {
                clear_thumbnail_preview("Thumbnail load failed");
                return;
            }

            QPixmap pixmap;
            if (!pixmap.loadFromData(data)) {
                clear_thumbnail_preview("Thumbnail decode failed");
                return;
            }

            original_thumbnail_pixmap_ = pixmap;
            current_thumbnail_url_ = reply->url().toString();
            apply_scaled_thumbnail_pixmap();
        });
    }

protected:
    void resizeEvent(QResizeEvent *event) override
    {
        QWidget::resizeEvent(event);

        if (original_thumbnail_pixmap_.isNull() || thumbnail_resize_timer_ == nullptr) {
            return;
        }

        thumbnail_resize_timer_->start(ThumbnailRescaleDebounceMs);
    }

private:
    void apply_category_selection(int index)
    {
        if (index < 0 || category_results_combo_ == nullptr || !category_results_combo_->isEnabled() || bridge_ == nullptr) {
            return;
        }

        bridge_->selectCategory(index);
        sync_detail_fields_from_bridge();
    }

    void refresh_category_results_combo()
    {
        if (category_results_combo_ == nullptr || bridge_ == nullptr) {
            return;
        }

        category_results_combo_->blockSignals(true);
        category_results_combo_->clear();

        const int item_count = bridge_->getCategory_count();
        for (int index = 0; index < item_count; ++index) {
            const QString label = bridge_->categoryResultLabel(index);
            category_results_combo_->addItem(label);
        }

        if (category_results_combo_->count() == 0) {
            category_results_combo_->addItem("No results");
            category_results_combo_->setEnabled(false);
            category_results_combo_->setCurrentIndex(0);
            clear_thumbnail_preview("No thumbnail");
        } else {
            category_results_combo_->setEnabled(true);
            category_results_combo_->setCurrentIndex(0);
        }

        category_results_combo_->blockSignals(false);

        if (category_results_combo_->isEnabled()) {
            apply_category_selection(category_results_combo_->currentIndex());
        }
    }

    void sync_detail_fields_from_bridge()
    {
        if (bridge_ == nullptr) {
            return;
        }

        category_type_edit_->setText(bridge_->getCategory_type());
        category_id_edit_->setText(bridge_->getCategory_id());
        category_name_edit_->setText(bridge_->getCategory_name());
        sync_thumbnail_from_bridge();
    }
};

extern "C" void *obs_chzzk_live_dock_create_widget()
{
    return new LiveDockWidget();
}

extern "C" void obs_chzzk_live_dock_destroy_widget(void *widget)
{
    delete static_cast<QWidget *>(widget);
}
