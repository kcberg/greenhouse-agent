use std::any::TypeId;
use std::collections::hash_map::DefaultHasher;
use std::fmt::format;
use std::hash::{Hash, Hasher};
use include_dir::{include_dir, Dir};
use log::info;
use warp::http::{HeaderMap, Uri};
use warp::{Filter, Rejection, Reply};

pub(crate) fn static_routes(
    cors_origins: Vec<&str>,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    // Root health check route
    static UI_DIST: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/ui/dist");

    let cors = warp::cors()
        .allow_origins(cors_origins)
        .allow_methods(vec!["GET", "POST", "PUT", "DELETE"]);

    let root_path = warp::path!()
        .map(|| warp::redirect(Uri::from_static("index.html")))
        .with(warp::reply::with::header("content-type", "text/html"))
        .with(cors.clone())
        .boxed();

    let aroute = root_path.clone();
    info!("Asset count {}", UI_DIST.entries().iter().len());
    // let type_id = TypeId::of::<isize>();
    // let cache_etag = format!("{:?}", type_id);
    let asset_routes = UI_DIST
        .entries()
        .iter()
        // .filter(|entry| entry.path().is_file())
        .map(|entry| {
            let file = UI_DIST.get_file(entry.path()).unwrap();
            let mime_opt = mime_guess::from_path(file.path());
            info!("adding asset route {} ", file.path().to_str().unwrap());
            (file.path().to_str().unwrap(), file, mime_opt.first())
        })
        .filter(|(_, _, mime_opt)| mime_opt.is_some())
        .map(|(path, file, mime_opt)| (path, file, mime_opt.unwrap()))
        .fold(aroute.boxed(), |routes, (path, file, mime)| {
            let mut s = DefaultHasher::new();
            let x = file.contents();
            x.hash(&mut s);
            let y = s.finish();
            let route = warp::path(path)
                .and(warp::get())
                .map(|| file.contents())
                .with(warp::reply::with::header("content-type", mime.to_string()))
                .with(warp::reply::with::header("Etag", y))
                .with(warp::reply::with::header("cache-control", "public, max-age=31536000"))
                .with(cors.clone());
            routes.or(route).unify().boxed()
        });

    asset_routes.boxed()
}
