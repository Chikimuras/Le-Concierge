//! Property orchestration — bridges the HTTP handlers to the repo and
//! emits audit events. The handlers never touch the repo directly.

use serde_json::json;

use crate::{
    audit::{AuditEvent, AuditRepo},
    auth::{OrgId, UserId},
    properties::{
        domain::{CreatePropertyInput, Property, PropertyId, UpdatePropertyInput},
        error::PropertyError,
        repo::PropertyRepo,
    },
};

#[derive(Clone)]
pub struct PropertyService {
    repo: PropertyRepo,
    audit: AuditRepo,
}

impl PropertyService {
    #[must_use]
    pub fn new(repo: PropertyRepo, audit: AuditRepo) -> Self {
        Self { repo, audit }
    }

    pub async fn list(&self, org_id: OrgId) -> Result<Vec<Property>, PropertyError> {
        self.repo.list(org_id).await
    }

    pub async fn get(&self, org_id: OrgId, id: PropertyId) -> Result<Property, PropertyError> {
        self.repo
            .find(org_id, id)
            .await?
            .ok_or(PropertyError::NotFound)
    }

    pub async fn create(
        &self,
        actor: UserId,
        org_id: OrgId,
        input: CreatePropertyInput,
    ) -> Result<Property, PropertyError> {
        let property = self.repo.create(org_id, &input).await?;
        self.emit_audit(AuditEvent {
            kind: "property.created",
            actor_user_id: Some(actor),
            org_id: Some(org_id),
            payload: json!({
                "property_id": property.id,
                "slug": property.slug.as_str(),
            }),
        })
        .await;
        Ok(property)
    }

    pub async fn update(
        &self,
        actor: UserId,
        org_id: OrgId,
        id: PropertyId,
        patch: UpdatePropertyInput,
    ) -> Result<Property, PropertyError> {
        let property = self.repo.update(org_id, id, &patch).await?;
        self.emit_audit(AuditEvent {
            kind: "property.updated",
            actor_user_id: Some(actor),
            org_id: Some(org_id),
            payload: json!({
                "property_id": property.id,
                "slug": property.slug.as_str(),
            }),
        })
        .await;
        Ok(property)
    }

    pub async fn delete(
        &self,
        actor: UserId,
        org_id: OrgId,
        id: PropertyId,
    ) -> Result<(), PropertyError> {
        self.repo.soft_delete(org_id, id).await?;
        self.emit_audit(AuditEvent {
            kind: "property.deleted",
            actor_user_id: Some(actor),
            org_id: Some(org_id),
            payload: json!({ "property_id": id }),
        })
        .await;
        Ok(())
    }

    async fn emit_audit(&self, event: AuditEvent) {
        if let Err(err) = self.audit.record(event).await {
            tracing::error!(error = %err, "failed to record property audit event");
        }
    }
}
