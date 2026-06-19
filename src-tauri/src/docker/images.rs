//! Image operations via the Docker Engine API.

use bollard::image::{
    CreateImageOptions, ListImagesOptions, PruneImagesOptions, RemoveImageOptions,
    SearchImagesOptions,
};
use bollard::Docker;
use futures_util::Stream;

use super::types::{ImageDto, SearchResultDto};
use crate::error::AppResult;

/// Search Docker Hub via the engine (`/images/search`). Returns repos (not tags).
pub async fn search(docker: &Docker, term: &str, limit: i64) -> AppResult<Vec<SearchResultDto>> {
    let opts = SearchImagesOptions::<String> {
        term: term.to_string(),
        limit: Some(limit.max(0) as u64),
        ..Default::default()
    };
    let res = docker.search_images(opts).await?;
    Ok(res
        .into_iter()
        .map(|r| SearchResultDto {
            name: r.name.unwrap_or_default(),
            description: r.description.unwrap_or_default(),
            stars: r.star_count.unwrap_or(0),
            official: r.is_official.unwrap_or(false),
        })
        .collect())
}

pub async fn list(docker: &Docker) -> AppResult<Vec<ImageDto>> {
    let opts = ListImagesOptions::<String> {
        all: false,
        ..Default::default()
    };
    let imgs = docker.list_images(Some(opts)).await?;
    Ok(imgs.into_iter().map(map_image).collect())
}

fn map_image(i: bollard::models::ImageSummary) -> ImageDto {
    let dangling = i.repo_tags.is_empty() || i.repo_tags.iter().all(|t| t == "<none>:<none>");
    ImageDto {
        id: i.id,
        tags: i.repo_tags,
        size: i.size,
        created: i.created,
        dangling,
    }
}

/// Pull `image:tag` from a registry; returns a stream of per-layer progress.
pub fn pull_stream(
    docker: &Docker,
    image: &str,
    tag: &str,
) -> impl Stream<Item = Result<bollard::models::CreateImageInfo, bollard::errors::Error>> {
    let opts = CreateImageOptions::<String> {
        from_image: image.to_string(),
        tag: tag.to_string(),
        ..Default::default()
    };
    docker.create_image(Some(opts), None, None)
}

pub async fn remove(docker: &Docker, id: &str, force: bool) -> AppResult<()> {
    let opts = RemoveImageOptions {
        force,
        noprune: false,
    };
    docker.remove_image(id, Some(opts), None).await?;
    Ok(())
}

pub async fn prune_dangling(docker: &Docker) -> AppResult<serde_json::Value> {
    let r = docker
        .prune_images(None::<PruneImagesOptions<String>>)
        .await?;
    Ok(serde_json::to_value(r)?)
}

pub async fn history(docker: &Docker, id: &str) -> AppResult<serde_json::Value> {
    let h = docker.image_history(id).await?;
    Ok(serde_json::to_value(h)?)
}

pub async fn inspect(docker: &Docker, id: &str) -> AppResult<serde_json::Value> {
    let r = docker.inspect_image(id).await?;
    Ok(serde_json::to_value(r)?)
}
