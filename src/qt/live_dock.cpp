#include <algorithm>
#include <QByteArray>
#include <QComboBox>
#include <QFrame>
#include <QHBoxLayout>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonParseError>
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
#include <QStringList>
#include <QTimer>
#include <QUrl>
#include <QVector>
#include <QVBoxLayout>
#include <QWidget>

extern "C" {
char *obs_chzzk_live_dock_load_current_json();
char *obs_chzzk_live_dock_search_category_json(const char *query);
char *obs_chzzk_live_dock_apply_update_json(
    const char *live_title,
    const char *category_type,
    const char *category_id,
    const char *tags);
char *obs_chzzk_live_dock_clear_category_json();
char *obs_chzzk_live_dock_clear_tags_json();
void obs_chzzk_live_dock_free_json(char *json_text);
}

class LiveDockWidget final : public QWidget {
public:
    struct CategoryResultItem {
        QString category_name;
        QString category_type;
        QString category_id;
        QString poster_image_url;
    };

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

        thumbnail_network_manager_ = new QNetworkAccessManager(this);
        thumbnail_resize_timer_ = new QTimer(this);
        thumbnail_resize_timer_->setSingleShot(true);
        connect(thumbnail_resize_timer_, &QTimer::timeout, this, [this]() {
            apply_scaled_thumbnail_pixmap();
        });

        connect(load_button, &QPushButton::clicked, this, [this]() {
            apply_response(call_load_current());
        });

        connect(search_button, &QPushButton::clicked, this, [this]() {
            const QString query = category_query_edit_->text().trimmed();
            if (query.isEmpty()) {
                status_label_->setText("Category Search Query is empty");
                return;
            }
            apply_response(call_search_category(query));
        });

        connect(category_results_combo_, qOverload<int>(&QComboBox::currentIndexChanged), this, [this](int index) {
            apply_category_selection(index);
        });

        connect(sort_mode_combo_, qOverload<int>(&QComboBox::currentIndexChanged), this, [this](int) {
            refresh_category_results_combo();
        });

        connect(apply_button, &QPushButton::clicked, this, [this]() {
            apply_response(call_apply_update(
                live_title_edit_->text().trimmed(),
                category_type_edit_->text().trimmed(),
                category_id_edit_->text().trimmed(),
                tags_edit_->text().trimmed()));
        });

        connect(clear_button, &QPushButton::clicked, this, [this]() {
            apply_response(call_clear_category());
        });

        connect(clear_tags_button, &QPushButton::clicked, this, [this]() {
            apply_response(call_clear_tags());
        });
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
    QNetworkAccessManager *thumbnail_network_manager_ = nullptr;
    QNetworkReply *thumbnail_reply_ = nullptr;
    QTimer *thumbnail_resize_timer_ = nullptr;
    QVector<CategoryResultItem> category_results_;
    QPixmap original_thumbnail_pixmap_;
    QString current_thumbnail_url_;

    static constexpr int CategoryTypeRole = 0x0100;
    static constexpr int CategoryIdRole = 0x0101;
    static constexpr int CategoryNameRole = 0x0102;
    static constexpr int CategoryPosterRole = 0x0103;
    static constexpr int ThumbnailRescaleDebounceMs = 40;

    static int category_relevance_score(const QString &query_text, const QString &name_text)
    {
        const QString query = query_text.trimmed().toLower();
        const QString name = name_text.trimmed().toLower();

        if (query.isEmpty() || name.isEmpty()) {
            return 0;
        }
        if (name == query) {
            return 10000;
        }
        if (name.startsWith(query)) {
            const int length_penalty = static_cast<int>(name.size() - query.size());
            return 8000 - std::max(0, length_penalty);
        }

        const int index = name.indexOf(query);
        if (index >= 0) {
            return 6000 - index;
        }

        return 0;
    }

    bool use_name_sort() const
    {
        if (sort_mode_combo_ == nullptr) {
            return false;
        }

        return sort_mode_combo_->currentData().toString() == QStringLiteral("name");
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
    static QJsonObject decode_result(char *raw)
    {
        if (raw == nullptr) {
            QJsonObject fallback;
            fallback.insert("ok", false);
            fallback.insert("status", "Dock call returned null response");
            return fallback;
        }

        const QByteArray bytes(raw);
        obs_chzzk_live_dock_free_json(raw);

        QJsonParseError parse_error;
        const QJsonDocument doc = QJsonDocument::fromJson(bytes, &parse_error);
        if (parse_error.error != QJsonParseError::NoError || !doc.isObject()) {
            QJsonObject fallback;
            fallback.insert("ok", false);
            fallback.insert("status", "Failed to parse dock response JSON");
            return fallback;
        }

        return doc.object();
    }

    static const char *to_utf8_cstr(const QString &text, QByteArray &storage)
    {
        storage = text.toUtf8();
        return storage.constData();
    }

    QJsonObject call_load_current()
    {
        return decode_result(obs_chzzk_live_dock_load_current_json());
    }

    QJsonObject call_search_category(const QString &query)
    {
        QByteArray query_utf8;
        return decode_result(obs_chzzk_live_dock_search_category_json(to_utf8_cstr(query, query_utf8)));
    }

    QJsonObject call_apply_update(
        const QString &live_title,
        const QString &category_type,
        const QString &category_id,
        const QString &tags)
    {
        QByteArray title_utf8;
        QByteArray type_utf8;
        QByteArray id_utf8;
        QByteArray tags_utf8;

        return decode_result(obs_chzzk_live_dock_apply_update_json(
            to_utf8_cstr(live_title, title_utf8),
            to_utf8_cstr(category_type, type_utf8),
            to_utf8_cstr(category_id, id_utf8),
            to_utf8_cstr(tags, tags_utf8)));
    }

    QJsonObject call_clear_category()
    {
        return decode_result(obs_chzzk_live_dock_clear_category_json());
    }

    QJsonObject call_clear_tags()
    {
        return decode_result(obs_chzzk_live_dock_clear_tags_json());
    }

    void apply_category_selection(int index)
    {
        if (index < 0 || category_results_combo_ == nullptr || !category_results_combo_->isEnabled()) {
            return;
        }

        const QString category_name = category_results_combo_->itemData(index, CategoryNameRole).toString();
        const QString category_type = category_results_combo_->itemData(index, CategoryTypeRole).toString();
        const QString category_id = category_results_combo_->itemData(index, CategoryIdRole).toString();
        const QString poster_url = category_results_combo_->itemData(index, CategoryPosterRole).toString();

        if (!category_name.isEmpty()) {
            category_name_edit_->setText(category_name);
        }
        if (!category_type.isEmpty()) {
            category_type_edit_->setText(category_type);
        }
        if (!category_id.isEmpty()) {
            category_id_edit_->setText(category_id);
        }

        load_thumbnail_preview(poster_url);
    }

    void refresh_category_results_combo()
    {
        if (category_results_combo_ == nullptr || sort_mode_combo_ == nullptr) {
            return;
        }

        QVector<CategoryResultItem> sorted_items = category_results_;
        const QString query_text = category_query_edit_ != nullptr ? category_query_edit_->text() : QString();

        if (use_name_sort()) {
            std::sort(sorted_items.begin(), sorted_items.end(), [](const CategoryResultItem &lhs, const CategoryResultItem &rhs) {
                return QString::localeAwareCompare(lhs.category_name, rhs.category_name) < 0;
            });
        } else {
            std::sort(sorted_items.begin(), sorted_items.end(), [&query_text](const CategoryResultItem &lhs, const CategoryResultItem &rhs) {
                const int lhs_score = category_relevance_score(query_text, lhs.category_name);
                const int rhs_score = category_relevance_score(query_text, rhs.category_name);
                if (lhs_score != rhs_score) {
                    return lhs_score > rhs_score;
                }

                return QString::localeAwareCompare(lhs.category_name, rhs.category_name) < 0;
            });
        }

        category_results_combo_->blockSignals(true);
        category_results_combo_->clear();

        for (const CategoryResultItem &item : sorted_items) {
            const QString label = QStringLiteral("%1 (%2)").arg(item.category_name, item.category_type);
            category_results_combo_->addItem(label);
            const int index = category_results_combo_->count() - 1;
            category_results_combo_->setItemData(index, item.category_type, CategoryTypeRole);
            category_results_combo_->setItemData(index, item.category_id, CategoryIdRole);
            category_results_combo_->setItemData(index, item.category_name, CategoryNameRole);
            category_results_combo_->setItemData(index, item.poster_image_url, CategoryPosterRole);
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

    void populate_category_results(const QJsonArray &categories)
    {
        category_results_.clear();

        for (const QJsonValue &value : categories) {
            if (!value.isObject()) {
                continue;
            }

            const QJsonObject item = value.toObject();
            const QString category_name = item.value("categoryName").toString().trimmed();
            const QString category_type = item.value("categoryType").toString().trimmed();
            const QString category_id = item.value("categoryId").toString().trimmed();
            const QString poster_url = item.value("posterImageUrl").toString().trimmed();

            if (category_name.isEmpty() || category_type.isEmpty() || category_id.isEmpty()) {
                continue;
            }

            category_results_.push_back(CategoryResultItem {
                category_name,
                category_type,
                category_id,
                poster_url,
            });
        }

        refresh_category_results_combo();
    }

    void apply_response(const QJsonObject &response)
    {
        if (response.contains("categories") && response.value("categories").isArray()) {
            populate_category_results(response.value("categories").toArray());
        }

        if (response.contains("liveTitle")) {
            live_title_edit_->setText(response.value("liveTitle").toString());
        }
        if (response.contains("tags")) {
            const QJsonValue tags_value = response.value("tags");
            if (tags_value.isArray()) {
                QStringList tags;
                for (const QJsonValue &value : tags_value.toArray()) {
                    if (!value.isString()) {
                        continue;
                    }

                    const QString tag = value.toString().trimmed();
                    if (!tag.isEmpty()) {
                        tags.append(tag);
                    }
                }

                tags_edit_->setText(tags.join(", "));
            } else {
                tags_edit_->clear();
            }
        }
        if (response.contains("categoryType")) {
            category_type_edit_->setText(response.value("categoryType").toString());
        }
        if (response.contains("categoryId")) {
            category_id_edit_->setText(response.value("categoryId").toString());
        }
        if (response.contains("categoryName")) {
            category_name_edit_->setText(response.value("categoryName").toString());
        }
        if (response.contains("posterImageUrl")) {
            load_thumbnail_preview(response.value("posterImageUrl").toString());
        } else if (!response.contains("categories") && response.contains("categoryId")) {
            const QString category_id = response.value("categoryId").toString().trimmed();
            if (category_id.isEmpty()) {
                clear_thumbnail_preview("No thumbnail");
            } else {
                clear_thumbnail_preview("Thumbnail unavailable");
            }
        }

        const QString status = response.value("status").toString();
        if (!status.isEmpty()) {
            status_label_->setText(status);
        }
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
