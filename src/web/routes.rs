use askama::Template;

use axum::{
    http::StatusCode,
    response::Html,
    routing::get,
    Router,
};

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate<'a> {
    title: &'a str,
    active_page: &'a str,
}

#[derive(Template)]
#[template(path = "about.html")]
struct AboutTemplate<'a> {
    title: &'a str,
    active_page: &'a str,
}

pub fn router() -> Router {
    Router::new()
        .route("/", get(index))
        .route("/about", get(about))
        .route("/status", get(status))
}

async fn index() -> Result<Html<String>, StatusCode> {
    let template = DashboardTemplate {
        title: "BOREAL",
        active_page: "dashboard",
    };

    render_template(&template)
}

async fn about() -> Result<Html<String>, StatusCode> {
    let template = AboutTemplate {
        title: "About BOREAL",
        active_page: "about",
    };

    render_template(&template)
}

async fn status() -> &'static str {
    "BOREAL is running"
}

fn render_template<T>(
    template: &T,
) -> Result<Html<String>, StatusCode>
where
    T: Template,
{
    template
        .render()
        .map(Html)
        .map_err(|error| {
            eprintln!(
                "Unable to render HTML template: {error}"
            );

            StatusCode::INTERNAL_SERVER_ERROR
        })
}