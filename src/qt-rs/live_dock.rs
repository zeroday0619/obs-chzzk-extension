use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;

use crate::settings::{
    apply_live_update_response, clear_live_category_response, clear_live_tags_response,
    current_settings, load_live_setting_response, search_category_response, LiveDockCategoryEntry,
    LiveDockResponse,
};

#[derive(Clone, Default)]
struct CategoryResultItem {
    category_name: QString,
    category_type: QString,
    category_id: QString,
    poster_image_url: QString,
}

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qproperty(QString, auth_status)]
        #[qproperty(bool, linked)]
        #[qproperty(QString, status)]
        #[qproperty(QString, live_title)]
        #[qproperty(QString, tags_text)]
        #[qproperty(QString, category_type)]
        #[qproperty(QString, category_id)]
        #[qproperty(QString, category_name)]
        #[qproperty(QString, poster_image_url)]
        #[qproperty(i32, category_count)]
        type LiveDockBridge = super::LiveDockBridgeRust;

        #[qinvokable]
        #[cxx_name = "refreshAuthState"]
        fn refresh_auth_state(self: Pin<&mut LiveDockBridge>);

        #[qinvokable]
        #[cxx_name = "loadCurrent"]
        fn load_current(self: Pin<&mut LiveDockBridge>);

        #[qinvokable]
        #[cxx_name = "searchCategory"]
        fn search_category(self: Pin<&mut LiveDockBridge>, query: &QString, sort_by_name: bool);

        #[qinvokable]
        #[cxx_name = "applyUpdate"]
        fn apply_update(
            self: Pin<&mut LiveDockBridge>,
            live_title: &QString,
            category_type: &QString,
            category_id: &QString,
            tags_text: &QString,
        );

        #[qinvokable]
        #[cxx_name = "clearCategory"]
        fn clear_category(self: Pin<&mut LiveDockBridge>);

        #[qinvokable]
        #[cxx_name = "clearTags"]
        fn clear_tags(self: Pin<&mut LiveDockBridge>);

        #[qinvokable]
        #[cxx_name = "refreshCategoryResults"]
        fn refresh_category_results(
            self: Pin<&mut LiveDockBridge>,
            query: &QString,
            sort_by_name: bool,
        );

        #[qinvokable]
        #[cxx_name = "selectCategory"]
        fn select_category(self: Pin<&mut LiveDockBridge>, index: i32);

        #[qinvokable]
        #[cxx_name = "categoryResultLabel"]
        fn category_result_label(self: &LiveDockBridge, index: i32) -> QString;

        #[qinvokable]
        #[cxx_name = "categoryResultName"]
        fn category_result_name(self: &LiveDockBridge, index: i32) -> QString;

        #[qinvokable]
        #[cxx_name = "categoryResultType"]
        fn category_result_type(self: &LiveDockBridge, index: i32) -> QString;

        #[qinvokable]
        #[cxx_name = "categoryResultId"]
        fn category_result_id(self: &LiveDockBridge, index: i32) -> QString;

        #[qinvokable]
        #[cxx_name = "categoryResultPosterUrl"]
        fn category_result_poster_url(self: &LiveDockBridge, index: i32) -> QString;
    }
}

#[derive(Default)]
pub struct LiveDockBridgeRust {
    auth_status: QString,
    linked: bool,
    status: QString,
    live_title: QString,
    tags_text: QString,
    category_type: QString,
    category_id: QString,
    category_name: QString,
    poster_image_url: QString,
    category_count: i32,
    category_results: Vec<CategoryResultItem>,
    sorted_category_indices: Vec<usize>,
}

impl qobject::LiveDockBridge {
    fn refresh_auth_state(mut self: Pin<&mut Self>) {
        let settings = current_settings();
        let linked = !settings.chzzk_authorization_token.trim().is_empty();
        let auth_status = if settings.chzzk_auth_status.trim().is_empty() {
            if linked {
                "CHZZK account linked"
            } else {
                "CHZZK account not linked"
            }
        } else {
            settings.chzzk_auth_status.as_str()
        };

        self.as_mut().set_linked(linked);
        self.set_auth_status(auth_status.into());
    }

    fn load_current(mut self: Pin<&mut Self>) {
        let response = load_live_setting_response("Loaded current live setting");
        self.as_mut().apply_result(response, None);
    }

    fn search_category(mut self: Pin<&mut Self>, query: &QString, sort_by_name: bool) {
        let query_text = query.to_string();
        if query_text.trim().is_empty() {
            self.as_mut().refresh_auth_state();
            self.as_mut()
                .set_status("Category Search Query is empty".into());
            self.as_mut().replace_category_results(Vec::new());
            self.as_mut().set_category_count(0);
            self.as_mut().clear_selected_category("No thumbnail");
            return;
        }
        let response = search_category_response(&query_text);
        self.as_mut()
            .apply_result(response, Some((query_text.as_str(), sort_by_name)));
    }

    fn apply_update(
        mut self: Pin<&mut Self>,
        live_title: &QString,
        category_type: &QString,
        category_id: &QString,
        tags_text: &QString,
    ) {
        let response = apply_live_update_response(
            &live_title.to_string(),
            &category_type.to_string(),
            &category_id.to_string(),
            &tags_text.to_string(),
        );
        self.as_mut().apply_result(response, None);
    }

    fn clear_category(mut self: Pin<&mut Self>) {
        let response = clear_live_category_response();
        self.as_mut().apply_result(response, None);
    }

    fn clear_tags(mut self: Pin<&mut Self>) {
        let response = clear_live_tags_response();
        self.as_mut().apply_result(response, None);
    }

    fn refresh_category_results(mut self: Pin<&mut Self>, query: &QString, sort_by_name: bool) {
        self.as_mut()
            .rebuild_sorted_results(&query.to_string(), sort_by_name);

        if self.category_count > 0 {
            self.select_category(0);
        } else {
            self.clear_selected_category("No thumbnail");
        }
    }

    fn select_category(mut self: Pin<&mut Self>, index: i32) {
        let selected = self.selected_category(index).cloned();
        if let Some(item) = selected {
            self.as_mut().set_category_name(item.category_name.clone());
            self.as_mut().set_category_type(item.category_type.clone());
            self.as_mut().set_category_id(item.category_id.clone());
            self.as_mut()
                .set_poster_image_url(item.poster_image_url.clone());
        }
    }

    fn category_result_label(&self, index: i32) -> QString {
        let Some(item) = self.selected_category(index) else {
            return QString::default();
        };

        format!("{} ({})", item.category_name, item.category_type)
            .as_str()
            .into()
    }

    fn category_result_name(&self, index: i32) -> QString {
        self.selected_category(index)
            .map(|item| item.category_name.clone())
            .unwrap_or_default()
    }

    fn category_result_type(&self, index: i32) -> QString {
        self.selected_category(index)
            .map(|item| item.category_type.clone())
            .unwrap_or_default()
    }

    fn category_result_id(&self, index: i32) -> QString {
        self.selected_category(index)
            .map(|item| item.category_id.clone())
            .unwrap_or_default()
    }

    fn category_result_poster_url(&self, index: i32) -> QString {
        self.selected_category(index)
            .map(|item| item.poster_image_url.clone())
            .unwrap_or_default()
    }
}

impl qobject::LiveDockBridge {
    fn apply_result(
        mut self: Pin<&mut Self>,
        response: Result<LiveDockResponse, String>,
        sort: Option<(&str, bool)>,
    ) {
        match response {
            Ok(response) => self.as_mut().apply_response(response, sort),
            Err(error) => {
                self.as_mut().refresh_auth_state();
                self.set_status(error.as_str().into());
            }
        }
    }

    fn apply_response(
        mut self: Pin<&mut Self>,
        response: LiveDockResponse,
        sort: Option<(&str, bool)>,
    ) {
        self.as_mut().refresh_auth_state();

        if let Some(live_title) = response.live_title.as_deref() {
            self.as_mut().set_live_title(live_title.into());
        }
        if let Some(tags) = response.tags {
            self.as_mut().set_tags_text(tags.join(", ").as_str().into());
        }
        if let Some(category_type) = response.category_type.as_deref() {
            self.as_mut().set_category_type(category_type.into());
        }
        if let Some(category_id) = response.category_id.as_deref() {
            self.as_mut().set_category_id(category_id.into());
        }
        if let Some(category_name) = response.category_name.as_deref() {
            self.as_mut().set_category_name(category_name.into());
        }

        if let Some(poster_image_url) = response.poster_image_url.as_deref() {
            self.as_mut().set_poster_image_url(poster_image_url.into());
        } else if response.category_id.is_some() && response.categories.is_none() {
            self.as_mut().set_poster_image_url(QString::default());
        }

        if let Some(categories) = response.categories {
            self.as_mut().replace_category_results(categories);
            let (query, sort_by_name) = sort.unwrap_or(("", false));
            self.as_mut().rebuild_sorted_results(query, sort_by_name);
            if self.category_count > 0 {
                self.as_mut().select_category(0);
            } else {
                self.as_mut().clear_selected_category("No thumbnail");
            }
        }

        self.as_mut().set_status(response.status.as_str().into());
    }

    fn replace_category_results(mut self: Pin<&mut Self>, categories: Vec<LiveDockCategoryEntry>) {
        let rust = unsafe { self.as_mut().rust_mut().get_unchecked_mut() };
        rust.category_results = categories
            .into_iter()
            .map(|item| CategoryResultItem {
                category_name: item.category_name.as_str().into(),
                category_type: item.category_type.as_str().into(),
                category_id: item.category_id.as_str().into(),
                poster_image_url: item.poster_image_url.unwrap_or_default().as_str().into(),
            })
            .collect();
    }

    fn rebuild_sorted_results(mut self: Pin<&mut Self>, query: &str, sort_by_name: bool) {
        let category_count = {
            let rust = unsafe { self.as_mut().rust_mut().get_unchecked_mut() };
            rust.sorted_category_indices = (0..rust.category_results.len()).collect();

            if sort_by_name {
                rust.sorted_category_indices.sort_by(|lhs, rhs| {
                    let lhs_name = rust.category_results[*lhs].category_name.to_string();
                    let rhs_name = rust.category_results[*rhs].category_name.to_string();
                    lhs_name.locale_cmp(&rhs_name)
                });
            } else {
                rust.sorted_category_indices.sort_by(|lhs, rhs| {
                    let lhs_name = rust.category_results[*lhs].category_name.to_string();
                    let rhs_name = rust.category_results[*rhs].category_name.to_string();
                    let lhs_score = category_relevance_score(query, &lhs_name);
                    let rhs_score = category_relevance_score(query, &rhs_name);
                    rhs_score
                        .cmp(&lhs_score)
                        .then_with(|| lhs_name.locale_cmp(&rhs_name))
                });
            }

            rust.sorted_category_indices.len() as i32
        };

        self.as_mut().set_category_count(category_count);
    }

    fn clear_selected_category(mut self: Pin<&mut Self>, _thumbnail_message: &str) {
        self.as_mut().set_category_name(QString::default());
        self.as_mut().set_category_type(QString::default());
        self.as_mut().set_category_id(QString::default());
        self.set_poster_image_url(QString::default());
    }

    fn selected_category(&self, index: i32) -> Option<&CategoryResultItem> {
        if index < 0 {
            return None;
        }

        let rust = self.rust();
        let sorted_index = rust.sorted_category_indices.get(index as usize)?;
        rust.category_results.get(*sorted_index)
    }
}

fn category_relevance_score(query_text: &str, name_text: &str) -> i32 {
    let query = query_text.trim().to_lowercase();
    let name = name_text.trim().to_lowercase();

    if query.is_empty() || name.is_empty() {
        return 0;
    }
    if name == query {
        return 10000;
    }
    if name.starts_with(&query) {
        let length_penalty = name.len().saturating_sub(query.len()) as i32;
        return 8000 - length_penalty.max(0);
    }
    if let Some(index) = name.find(&query) {
        return 6000 - index as i32;
    }

    0
}

trait LocaleCmp {
    fn locale_cmp(&self, other: &str) -> core::cmp::Ordering;
}

impl LocaleCmp for String {
    fn locale_cmp(&self, other: &str) -> core::cmp::Ordering {
        self.to_lowercase().cmp(&other.to_lowercase())
    }
}
