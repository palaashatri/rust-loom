//! Provider registries: ordered registration, capability routing, and
//! per-capability run dispatch.

use std::sync::{Arc, RwLock};

use crate::error::VisionError;
use crate::provider::{
    CapabilityId, CapabilityProvider, ProviderInput, ProviderOutput, RunContext,
};

/// Ordered registry of capability providers.
///
/// Registration order is significant: [`ProviderRegistry::best_for`] returns
/// the first provider registered for a capability, so a caller can shadow a
/// built-in provider by registering a preferred one first.
///
/// Lookup methods return owned [`Arc`] handles rather than borrowed trait
/// objects, which keeps the registry sound without `unsafe`: the internal
/// lock is released before handles escape, and every handle keeps its
/// provider alive. Handles are cheap to clone.
pub struct ProviderRegistry {
    providers: RwLock<Vec<Arc<dyn CapabilityProvider>>>,
}

impl ProviderRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        ProviderRegistry {
            providers: RwLock::new(Vec::new()),
        }
    }

    /// Registers a provider, appending it after all existing providers.
    pub fn register(&self, provider: Box<dyn CapabilityProvider>) {
        self.providers.write().unwrap().push(Arc::from(provider));
    }

    /// Registers a shared provider handle, appending it after all existing
    /// providers.
    pub fn register_arc(&self, provider: Arc<dyn CapabilityProvider>) {
        self.providers.write().unwrap().push(provider);
    }

    /// Returns every provider that implements `capability_id`, in
    /// registration order.
    pub fn providers_for(&self, capability_id: CapabilityId) -> Vec<Arc<dyn CapabilityProvider>> {
        let guard = self.providers.read().unwrap();
        guard
            .iter()
            .filter(|p| p.descriptor().capability_id == capability_id)
            .cloned()
            .collect()
    }

    /// Returns the first registered provider for `capability_id`, if any.
    ///
    /// "First registered wins" by design: registering a preferred provider
    /// before the defaults changes which one is returned.
    pub fn best_for(&self, capability_id: CapabilityId) -> Option<Arc<dyn CapabilityProvider>> {
        self.providers_for(capability_id).into_iter().next()
    }

    /// Removes every provider that declares `capability_id` and returns how
    /// many were removed.
    ///
    /// Note that a provider declares exactly one capability in its
    /// descriptor, so this removes the whole provider(s).
    pub fn unregister(&self, capability_id: CapabilityId) -> usize {
        let mut guard = self.providers.write().unwrap();
        let before = guard.len();
        guard.retain(|p| p.descriptor().capability_id != capability_id);
        before - guard.len()
    }

    /// Returns the total number of registered providers.
    pub fn len(&self) -> usize {
        self.providers.read().unwrap().len()
    }

    /// Returns whether no providers are registered.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-capability routing to every provider that supports a capability.
///
/// Wraps a [`ProviderRegistry`] and adds run dispatch helpers used by
/// applications that want to try multiple providers in order.
pub struct CapabilityRegistry {
    inner: ProviderRegistry,
}

impl CapabilityRegistry {
    /// Creates an empty capability registry.
    pub fn new() -> Self {
        CapabilityRegistry {
            inner: ProviderRegistry::new(),
        }
    }

    /// Registers a provider (see [`ProviderRegistry::register`]).
    pub fn register(&self, provider: Box<dyn CapabilityProvider>) {
        self.inner.register(provider);
    }

    /// Registers a shared provider handle.
    pub fn register_arc(&self, provider: Arc<dyn CapabilityProvider>) {
        self.inner.register_arc(provider);
    }

    /// Returns the underlying ordered registry.
    pub fn providers(&self) -> &ProviderRegistry {
        &self.inner
    }

    /// Runs `input` through every provider that supports `capability_id` and
    /// collects each result, in registration order.
    ///
    /// Cancellation and progress apply to the whole loop through `ctx`; a
    /// provider may consume progress values before the next provider runs.
    pub fn run_all(
        &self,
        capability_id: CapabilityId,
        input: &ProviderInput,
        ctx: &mut RunContext,
    ) -> Vec<Result<ProviderOutput, VisionError>> {
        self.inner
            .providers_for(capability_id)
            .iter()
            .map(|provider| provider.run(input, ctx))
            .collect()
    }

    /// Runs `input` through the providers for `capability_id` until one
    /// succeeds, returning the first successful output.
    ///
    /// If every provider fails, the last error is returned; if no provider is
    /// registered, [`VisionError::ProviderUnavailable`] is returned.
    pub fn run_first_success(
        &self,
        capability_id: CapabilityId,
        input: &ProviderInput,
        ctx: &mut RunContext,
    ) -> Result<ProviderOutput, VisionError> {
        let mut last_error = Err(VisionError::ProviderUnavailable(format!(
            "no provider registered for capability '{capability_id}'"
        )));
        for provider in self.inner.providers_for(capability_id) {
            match provider.run(input, ctx) {
                Ok(output) => return Ok(output),
                Err(err) => last_error = Err(err),
            }
        }
        last_error
    }
}

impl Default for CapabilityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::ProviderDescriptor;

    struct DummyProvider {
        descriptor: ProviderDescriptor,
    }

    impl DummyProvider {
        fn new(capability_id: CapabilityId, name: &str) -> Self {
            let mut descriptor = ProviderDescriptor::new(capability_id);
            descriptor.name = name.to_string();
            DummyProvider { descriptor }
        }
    }

    impl CapabilityProvider for DummyProvider {
        fn descriptor(&self) -> &ProviderDescriptor {
            &self.descriptor
        }

        fn run(
            &self,
            input: &ProviderInput,
            ctx: &mut RunContext,
        ) -> Result<ProviderOutput, VisionError> {
            ctx.check_cancelled()?;
            match input {
                ProviderInput::Text { text } => Ok(ProviderOutput::Generic {
                    message: format!("{}:{text}", self.descriptor.name),
                }),
                _ => Err(VisionError::UnsupportedInput),
            }
        }
    }

    fn boxed(capability_id: CapabilityId, name: &str) -> Box<dyn CapabilityProvider> {
        Box::new(DummyProvider::new(capability_id, name))
    }

    #[test]
    fn best_for_returns_first_registered() {
        let registry = ProviderRegistry::new();
        registry.register(boxed(CapabilityId::Ocr, "first"));
        registry.register(boxed(CapabilityId::Ocr, "second"));

        let best = registry.best_for(CapabilityId::Ocr).expect("provider");
        assert_eq!(best.descriptor().name, "first");
    }

    #[test]
    fn providers_for_returns_all_in_order() {
        let registry = ProviderRegistry::new();
        registry.register(boxed(CapabilityId::Ocr, "first"));
        registry.register(boxed(CapabilityId::ImageStats, "stats"));
        registry.register(boxed(CapabilityId::Ocr, "second"));

        let names: Vec<String> = registry
            .providers_for(CapabilityId::Ocr)
            .iter()
            .map(|p| p.descriptor().name.clone())
            .collect();
        assert_eq!(names, vec!["first".to_string(), "second".to_string()]);
    }

    #[test]
    fn providers_for_never_returns_other_capabilities() {
        let registry = ProviderRegistry::new();
        registry.register(boxed(CapabilityId::Ocr, "ocr-only"));
        registry.register(boxed(CapabilityId::Tracking, "tracker"));
        assert_eq!(registry.providers_for(CapabilityId::Tracking).len(), 1);
        assert_eq!(registry.providers_for(CapabilityId::Ocr).len(), 1);
        assert_eq!(registry.providers_for(CapabilityId::Matting).len(), 0);
    }

    #[test]
    fn best_for_returns_none_when_unregistered() {
        let registry = ProviderRegistry::new();
        assert!(registry.best_for(CapabilityId::Pose).is_none());
        assert!(registry.is_empty());
    }

    #[test]
    fn unregister_removes_every_provider_for_capability() {
        let registry = ProviderRegistry::new();
        registry.register(boxed(CapabilityId::Ocr, "first"));
        registry.register(boxed(CapabilityId::Ocr, "second"));
        registry.register(boxed(CapabilityId::ImageStats, "stats"));

        assert_eq!(registry.unregister(CapabilityId::Ocr), 2);
        assert_eq!(registry.len(), 1);
        assert_eq!(registry.providers_for(CapabilityId::Ocr).len(), 0);
        assert!(registry.best_for(CapabilityId::Ocr).is_none());
    }

    #[test]
    fn unregister_of_unknown_capability_removes_nothing() {
        let registry = ProviderRegistry::new();
        registry.register(boxed(CapabilityId::Ocr, "first"));
        assert_eq!(registry.unregister(CapabilityId::Barcode), 0);
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn register_after_unregister_changes_best() {
        let registry = ProviderRegistry::new();
        registry.register(boxed(CapabilityId::Ocr, "first"));
        registry.unregister(CapabilityId::Ocr);
        registry.register(boxed(CapabilityId::Ocr, "replacement"));
        assert_eq!(
            registry
                .best_for(CapabilityId::Ocr)
                .unwrap()
                .descriptor()
                .name,
            "replacement"
        );
    }

    #[test]
    fn handles_keep_providers_alive() {
        let registry = ProviderRegistry::new();
        registry.register(boxed(CapabilityId::Ocr, "first"));
        let handle = registry.best_for(CapabilityId::Ocr).unwrap();
        registry.unregister(CapabilityId::Ocr);
        // The handle outlives unregistration.
        assert_eq!(handle.descriptor().name, "first");
    }

    #[test]
    fn run_all_collects_every_result() {
        let registry = CapabilityRegistry::new();
        registry.register(boxed(CapabilityId::Ocr, "first"));
        registry.register(boxed(CapabilityId::Ocr, "second"));

        let input = ProviderInput::Text {
            text: "hi".to_string(),
        };
        let mut ctx = RunContext::new();
        let results = registry.run_all(CapabilityId::Ocr, &input, &mut ctx);
        assert_eq!(results.len(), 2);
        assert!(
            matches!(&results[0], Ok(ProviderOutput::Generic { message }) if message == "first:hi")
        );
        assert!(
            matches!(&results[1], Ok(ProviderOutput::Generic { message }) if message == "second:hi")
        );
    }

    #[test]
    fn run_first_success_skips_failing_providers() {
        let registry = CapabilityRegistry::new();
        registry.register(boxed(CapabilityId::Ocr, "fails-on-text"));
        registry.register(boxed(CapabilityId::Ocr, "succeeds"));

        let input = ProviderInput::Image {
            width: 1,
            height: 1,
            channels: 1,
            data: vec![0],
            format: "gray".to_string(),
        };
        let mut ctx = RunContext::new();
        // Both DummyProviders reject Image, so the first error is returned.
        let result = registry.run_first_success(CapabilityId::Ocr, &input, &mut ctx);
        assert!(matches!(result, Err(VisionError::UnsupportedInput)));

        // With Text input both succeed; the first registered wins.
        let input = ProviderInput::Text {
            text: "hi".to_string(),
        };
        let result = registry.run_first_success(CapabilityId::Ocr, &input, &mut ctx);
        assert!(
            matches!(result, Ok(ProviderOutput::Generic { message }) if message == "fails-on-text:hi")
        );
    }

    #[test]
    fn run_first_success_returns_unavailable_without_providers() {
        let registry = CapabilityRegistry::new();
        let input = ProviderInput::Text {
            text: "hi".to_string(),
        };
        let mut ctx = RunContext::new();
        let result = registry.run_first_success(CapabilityId::Pose, &input, &mut ctx);
        assert!(matches!(result, Err(VisionError::ProviderUnavailable(_))));
    }

    #[test]
    fn cancelled_context_propagates_through_registry() {
        let registry = CapabilityRegistry::new();
        registry.register(boxed(CapabilityId::Ocr, "first"));
        let input = ProviderInput::Text {
            text: "hi".to_string(),
        };
        let mut ctx = RunContext::new();
        ctx.cancel();
        let result = registry.run_first_success(CapabilityId::Ocr, &input, &mut ctx);
        assert!(matches!(result, Err(VisionError::Cancelled)));
    }
}
