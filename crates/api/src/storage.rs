//! The object store: sheet PDFs and workspace assets.
//!
//! Bytes never pass through this server. The client is handed a presigned URL and talks to the
//! store directly, which is what keeps a 40 MB score off the API's memory and off its timeout.

use std::time::Duration;

use aws_sdk_s3::Client;
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};

use crate::config::Storage;
use crate::error::{ApiError, ApiResult};

#[derive(Clone, Debug)]
pub struct ObjectStore {
    client: Client,
    bucket: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredObject {
    pub key: String,
    pub size: i64,
    pub modified_ms: i64,
}

impl ObjectStore {
    pub async fn new(config: &Storage) -> ObjectStore {
        let credentials = aws_credential_types::Credentials::new(
            config.key.clone(),
            config.secret.clone(),
            None,
            None,
            "aurum-config",
        );

        let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_config::Region::new(config.region.clone()))
            .credentials_provider(credentials);

        if let Some(endpoint) = &config.endpoint {
            loader = loader.endpoint_url(endpoint);
        }

        let shared = loader.load().await;
        let mut builder = aws_sdk_s3::config::Builder::from(&shared);

        // MinIO and most self-hosted stores address buckets by path, not by subdomain.
        builder.set_force_path_style(Some(config.use_path_style));

        ObjectStore {
            client: Client::from_conf(builder.build()),
            bucket: config.bucket.clone(),
        }
    }

    /// Size and stored checksum, or `None` when there is no such object.
    pub async fn head(&self, key: &str) -> Option<(i64, Option<String>)> {
        let head = self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .ok()?;

        Some((
            head.content_length().unwrap_or_default(),
            head.checksum_sha256().map(str::to_owned),
        ))
    }

    pub async fn exists(&self, key: &str) -> bool {
        self.head(key).await.is_some()
    }

    pub async fn presign_get(&self, key: &str, ttl_seconds: u64) -> ApiResult<String> {
        let request = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .presigned(presigning(ttl_seconds)?)
            .await
            .map_err(|error| ApiError::internal("presigning a download", error))?;

        Ok(request.uri().to_owned())
    }

    pub async fn create_multipart_upload(
        &self,
        key: &str,
        content_type: &str,
    ) -> ApiResult<String> {
        let created = self
            .client
            .create_multipart_upload()
            .bucket(&self.bucket)
            .key(key)
            .content_type(content_type)
            .send()
            .await
            .map_err(|error| ApiError::internal("starting an upload", error))?;

        created
            .upload_id()
            .map(str::to_owned)
            .ok_or_else(|| ApiError::internal("starting an upload", "no upload id"))
    }

    pub async fn presign_upload_part(
        &self,
        key: &str,
        upload_id: &str,
        part_number: i32,
        ttl_seconds: u64,
    ) -> ApiResult<String> {
        let request = self
            .client
            .upload_part()
            .bucket(&self.bucket)
            .key(key)
            .upload_id(upload_id)
            .part_number(part_number)
            .presigned(presigning(ttl_seconds)?)
            .await
            .map_err(|error| ApiError::internal("presigning an upload part", error))?;

        Ok(request.uri().to_owned())
    }

    pub async fn complete_multipart_upload(
        &self,
        key: &str,
        upload_id: &str,
        parts: &[(i32, String)],
    ) -> ApiResult<()> {
        let completed = CompletedMultipartUpload::builder()
            .set_parts(Some(
                parts
                    .iter()
                    .map(|(number, etag)| {
                        CompletedPart::builder()
                            .part_number(*number)
                            .e_tag(etag)
                            .build()
                    })
                    .collect(),
            ))
            .build();

        self.client
            .complete_multipart_upload()
            .bucket(&self.bucket)
            .key(key)
            .upload_id(upload_id)
            .multipart_upload(completed)
            .send()
            .await
            .map_err(|error| ApiError::internal("completing an upload", error))?;

        Ok(())
    }

    /// Aborting an upload that already completed or expired is not a failure worth surfacing;
    /// the bucket's lifecycle rule collects whatever is left.
    pub async fn abort_multipart_upload(&self, key: &str, upload_id: &str) {
        let _ = self
            .client
            .abort_multipart_upload()
            .bucket(&self.bucket)
            .key(key)
            .upload_id(upload_id)
            .send()
            .await;
    }

    pub async fn delete(&self, key: &str) -> ApiResult<()> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|error| ApiError::internal("deleting an object", error))?;

        Ok(())
    }

    pub async fn list_prefix(&self, prefix: &str) -> ApiResult<Vec<StoredObject>> {
        let mut objects = Vec::new();
        let mut pages = self
            .client
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix(prefix)
            .into_paginator()
            .send();

        while let Some(page) = pages.next().await {
            let page = page.map_err(|error| ApiError::internal("listing objects", error))?;

            for object in page.contents() {
                objects.push(StoredObject {
                    key: object.key().unwrap_or_default().to_owned(),
                    size: object.size().unwrap_or_default(),
                    modified_ms: object
                        .last_modified()
                        .map(|when| when.to_millis().unwrap_or_default())
                        .unwrap_or_default(),
                });
            }
        }

        Ok(objects)
    }
}

fn presigning(ttl_seconds: u64) -> ApiResult<PresigningConfig> {
    PresigningConfig::expires_in(Duration::from_secs(ttl_seconds))
        .map_err(|error| ApiError::internal("presigning", error))
}
