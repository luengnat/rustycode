// rustycode-orchestra/src/namespaced_resolver.rs
//! Namespaced Resolver Module
//!
//! Implements context-aware resolution with three-tier lookup precedence:
//! 1. Canonical (fully-qualified names with `:`)
//! 2. Local-first (caller namespace + bare name)
//! 3. Shorthand (bare name matched across all namespaces)
//!
//! This is the core logic for D003 (same-plugin local-first) and R007/R008 (safe shorthand).

use std::sync::Arc;

// ============================================================================
// Type Definitions
// ============================================================================

/// Resolution context provided by the caller.
/// Used to enable local-first resolution within a namespace.
#[derive(Debug, Clone, Default)]
pub struct ResolutionContext {
    /// The namespace of the calling component (e.g., "farm" from "farm:caller")
    pub caller_namespace: Option<String>,
}

/// Component type enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ComponentType {
    Skill,
    Agent,
}

/// Base structure for all resolution results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolutionResultBase {
    /// The original name passed to resolve()
    pub requested_name: String,

    /// How the resolution was performed
    pub resolution: ResolutionType,
}

/// Resolution type enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ResolutionType {
    Canonical,
    Alias,
    LocalFirst,
    Shorthand,
    Ambiguous,
    NotFound,
}

/// Result when a canonical (fully-qualified) name matches exactly.
/// Example: "farm:call-horse" resolves directly to the component with that canonical name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalResolution {
    /// The original name passed to resolve()
    pub requested_name: String,

    /// How the resolution was performed
    pub resolution: ResolutionType,

    /// The matched component
    pub component: ResolverComponent,
}

/// Result when an alias resolves to a canonical name.
/// Example: "py3d" resolves via alias to "python-tools:3d-visualizer".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasResolution {
    /// The original name passed to resolve()
    pub requested_name: String,

    /// How the resolution was performed
    pub resolution: ResolutionType,

    /// The matched component
    pub component: ResolverComponent,

    /// The alias that was resolved
    pub alias: String,

    /// The canonical name the alias points to
    pub canonical_name: String,
}

/// Result when a bare name resolves via local-first lookup.
/// Example: A caller in namespace "farm" resolving bare "call-horse" matches "farm:call-horse".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalFirstResolution {
    /// The original name passed to resolve()
    pub requested_name: String,

    /// How the resolution was performed
    pub resolution: ResolutionType,

    /// The matched component
    pub component: ResolverComponent,

    /// The namespace used for local-first resolution
    pub matched_namespace: String,
}

/// Result when a bare name matches exactly one component across all namespaces.
/// Example: "feed-chickens" resolves if only "farm:feed-chickens" exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShorthandResolution {
    /// The original name passed to resolve()
    pub requested_name: String,

    /// How the resolution was performed
    pub resolution: ResolutionType,

    /// The matched component
    pub component: ResolverComponent,
}

/// Result when a bare name matches multiple components across namespaces.
/// Returns all candidates for diagnostic consumption without throwing.
/// Example: "call-horse" matches both "farm:call-horse" and "zoo:call-horse".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmbiguousResolution {
    /// The original name passed to resolve()
    pub requested_name: String,

    /// How the resolution was performed
    pub resolution: ResolutionType,

    /// All components matching the bare name
    pub candidates: Vec<ResolverComponent>,
}

/// Result when no component matches the requested name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotFoundResolution {
    /// The original name passed to resolve()
    pub requested_name: String,

    /// How the resolution was performed
    pub resolution: ResolutionType,
}

/// Discriminated union of all resolution results.
/// The `resolution` field indicates which variant applies.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NameResolutionResult {
    Canonical(CanonicalResolution),
    Alias(AliasResolution),
    LocalFirst(LocalFirstResolution),
    Shorthand(ShorthandResolution),
    Ambiguous(AmbiguousResolution),
    NotFound(NotFoundResolution),
}

impl NameResolutionResult {
    /// Get the requested name from any resolution result.
    pub fn requested_name(&self) -> &str {
        match self {
            NameResolutionResult::Canonical(r) => &r.requested_name,
            NameResolutionResult::Alias(r) => &r.requested_name,
            NameResolutionResult::LocalFirst(r) => &r.requested_name,
            NameResolutionResult::Shorthand(r) => &r.requested_name,
            NameResolutionResult::Ambiguous(r) => &r.requested_name,
            NameResolutionResult::NotFound(r) => &r.requested_name,
        }
    }

    /// Get the resolution type from any resolution result.
    pub fn resolution_type(&self) -> ResolutionType {
        match self {
            NameResolutionResult::Canonical(r) => r.resolution,
            NameResolutionResult::Alias(r) => r.resolution,
            NameResolutionResult::LocalFirst(r) => r.resolution,
            NameResolutionResult::Shorthand(r) => r.resolution,
            NameResolutionResult::Ambiguous(r) => r.resolution,
            NameResolutionResult::NotFound(r) => r.resolution,
        }
    }
}

// ============================================================================
// Namespaced Component (simplified for portability)
// ============================================================================

/// A component entry in the namespaced registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolverComponent {
    /// The component's local name (e.g., "code-review")
    pub name: String,

    /// The plugin namespace (e.g., "my-plugin"). None for flat components.
    pub namespace: Option<String>,

    /// The computed canonical identifier: `${namespace}:${name}` or bare `name`
    pub canonical_name: String,

    /// Component type: skill or agent
    pub component_type: ComponentType,

    /// Absolute path to the component's definition file
    pub file_path: String,

    /// Source identifier (e.g., "plugin:my-plugin", "user", "project")
    pub source: String,
}

// ============================================================================
// Registry Trait (for dependency injection)
// ============================================================================

/// Trait for registries that can be used by NamespacedResolver.
pub trait NamespacedRegistry: Send + Sync {
    /// Get a component by its canonical name.
    fn get_by_canonical(&self, canonical_name: &str) -> Option<ResolverComponent>;

    /// Resolve an alias to its canonical name.
    fn resolve_alias(&self, alias: &str) -> Option<String>;

    /// Get all registered components.
    fn get_all(&self) -> Vec<ResolverComponent>;
}

// ============================================================================
// NamespacedResolver
// ============================================================================

/// Resolver for namespaced components with context-aware lookup.
///
/// Implements four-tier resolution precedence:
/// 1. **Canonical**: If name contains `:`, try exact match → return canonical result
/// 2. **Alias**: If name is a registered alias → return alias result
/// 3. **Local-first**: If `context.caller_namespace` exists, try `${caller_namespace}:${name}` → return local-first result
/// 4. **Shorthand**: Scan all components for bare name match → single match returns shorthand, multiple returns ambiguous
///
/// # Examples
/// ```rust,no_run
/// use rustycode_orchestra::namespaced_resolver::{
///     NamespacedResolver, ResolutionResult, ResolutionContext,
/// };
///
/// // Canonical lookup
/// let canon = resolver.resolve("farm:call-horse", None, None);
/// assert!(matches!(canon, NameResolutionResult::Canonical(_)));
///
/// // Local-first resolution from caller context
/// let local = resolver.resolve(
///     "call-horse",
///     Some(ResolutionContext {
///         caller_namespace: Some("farm".to_string()),
///     }),
///     None,
/// );
/// assert!(matches!(local, NameResolutionResult::LocalFirst(_)));
///
/// // Unambiguous shorthand
/// let short = resolver.resolve("unique-skill", None, None);
/// assert!(matches!(short, NameResolutionResult::Shorthand(_)));
///
/// // Ambiguous shorthand
/// let amb = resolver.resolve("common-skill", None, None);
/// assert!(matches!(amb, NameResolutionResult::Ambiguous(_)));
/// ```
pub struct NamespacedResolver<R> {
    /// The registry to resolve against
    registry: Arc<R>,
}

impl<R> NamespacedResolver<R>
where
    R: NamespacedRegistry,
{
    /// Create a new resolver for the given registry.
    ///
    /// # Arguments
    /// * `registry` - The namespaced registry to resolve against
    pub fn new(registry: R) -> Self {
        Self {
            registry: Arc::new(registry),
        }
    }

    /// Create a new resolver from an Arc-wrapped registry.
    ///
    /// # Arguments
    /// * `registry` - Arc-wrapped namespaced registry
    pub fn from_arc(registry: Arc<R>) -> Self {
        Self { registry }
    }

    /// Resolve a component name with context-aware lookup.
    ///
    /// Implements four-tier resolution precedence:
    /// 1. **Canonical**: If name contains `:`, try exact match → return canonical result
    /// 2. **Alias**: If name is a registered alias → return alias result
    /// 3. **Local-first**: If `context.caller_namespace` exists, try `${caller_namespace}:${name}` → return local-first result
    /// 4. **Shorthand**: Scan all components for bare name match → single match returns shorthand, multiple returns ambiguous
    ///
    /// # Arguments
    /// * `name` - The name to resolve (canonical or bare)
    /// * `context` - Optional resolution context with caller namespace
    /// * `component_type` - Optional type filter (skill or agent)
    ///
    /// # Returns
    /// Resolution result indicating how the match was found
    pub fn resolve(
        &self,
        name: &str,
        context: Option<&ResolutionContext>,
        component_type: Option<ComponentType>,
    ) -> NameResolutionResult {
        // Tier 1: Canonical lookup (name contains `:`)
        if name.contains(':') {
            if let Some(component) = self.registry.get_by_canonical(name) {
                if self.matches_type(&component, component_type) {
                    return NameResolutionResult::Canonical(CanonicalResolution {
                        requested_name: name.to_string(),
                        resolution: ResolutionType::Canonical,
                        component,
                    });
                }
            }

            // Canonical name not found
            return NameResolutionResult::NotFound(NotFoundResolution {
                requested_name: name.to_string(),
                resolution: ResolutionType::NotFound,
            });
        }

        // Tier 2: Alias lookup (before local-first and shorthand)
        if let Some(alias_target) = self.registry.resolve_alias(name) {
            if let Some(component) = self.registry.get_by_canonical(&alias_target) {
                if self.matches_type(&component, component_type) {
                    return NameResolutionResult::Alias(AliasResolution {
                        requested_name: name.to_string(),
                        resolution: ResolutionType::Alias,
                        component,
                        alias: name.to_string(),
                        canonical_name: alias_target,
                    });
                }
            }
        }

        // Tier 3: Local-first resolution (if caller namespace provided)
        if let Some(ctx) = context {
            if let Some(caller_ns) = &ctx.caller_namespace {
                let local_canonical = format!("{}:{}", caller_ns, name);
                if let Some(component) = self.registry.get_by_canonical(&local_canonical) {
                    if self.matches_type(&component, component_type) {
                        return NameResolutionResult::LocalFirst(LocalFirstResolution {
                            requested_name: name.to_string(),
                            resolution: ResolutionType::LocalFirst,
                            component,
                            matched_namespace: caller_ns.clone(),
                        });
                    }
                }
            }
        }

        // Tier 4: Shorthand resolution (scan all components)
        let candidates = self.find_bare_name_matches(name, component_type);

        if candidates.is_empty() {
            return NameResolutionResult::NotFound(NotFoundResolution {
                requested_name: name.to_string(),
                resolution: ResolutionType::NotFound,
            });
        }

        if candidates.len() == 1 {
            return NameResolutionResult::Shorthand(ShorthandResolution {
                requested_name: name.to_string(),
                resolution: ResolutionType::Shorthand,
                component: candidates.into_iter().next().unwrap(),
            });
        }

        // Multiple matches - ambiguous
        NameResolutionResult::Ambiguous(AmbiguousResolution {
            requested_name: name.to_string(),
            resolution: ResolutionType::Ambiguous,
            candidates,
        })
    }

    /// Find all components whose local name (without namespace) matches the given bare name.
    /// Optionally filters by component type.
    ///
    /// # Arguments
    /// * `bare_name` - The bare name to match
    /// * `component_type` - Optional type filter
    ///
    /// # Returns
    /// Array of matching components
    fn find_bare_name_matches(
        &self,
        bare_name: &str,
        component_type: Option<ComponentType>,
    ) -> Vec<ResolverComponent> {
        let all = self.registry.get_all();

        all.into_iter()
            .filter(|component| {
                // Match by local name (component.name)
                if component.name != bare_name {
                    return false;
                }

                // Apply type filter if provided
                self.matches_type(component, component_type)
            })
            .collect()
    }

    /// Check if a component matches the optional type filter.
    ///
    /// # Arguments
    /// * `component` - The component to check
    /// * `component_type` - Optional type filter
    ///
    /// # Returns
    /// true if no filter or type matches
    fn matches_type(
        &self,
        component: &ResolverComponent,
        component_type: Option<ComponentType>,
    ) -> bool {
        match component_type {
            None => true,
            Some(ct) => component.component_type == ct,
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // Mock registry for testing
    struct MockRegistry {
        components: HashMap<String, ResolverComponent>,
        aliases: HashMap<String, String>,
    }

    impl MockRegistry {
        fn new() -> Self {
            let mut components = HashMap::new();
            let aliases = HashMap::new();

            // Add some test components
            components.insert(
                "farm:call-horse".to_string(),
                ResolverComponent {
                    name: "call-horse".to_string(),
                    namespace: Some("farm".to_string()),
                    canonical_name: "farm:call-horse".to_string(),
                    component_type: ComponentType::Skill,
                    file_path: "/farm/call-horse.md".to_string(),
                    source: "plugin:farm".to_string(),
                },
            );

            components.insert(
                "zoo:call-horse".to_string(),
                ResolverComponent {
                    name: "call-horse".to_string(),
                    namespace: Some("zoo".to_string()),
                    canonical_name: "zoo:call-horse".to_string(),
                    component_type: ComponentType::Skill,
                    file_path: "/zoo/call-horse.md".to_string(),
                    source: "plugin:zoo".to_string(),
                },
            );

            components.insert(
                "farm:feed-chickens".to_string(),
                ResolverComponent {
                    name: "feed-chickens".to_string(),
                    namespace: Some("farm".to_string()),
                    canonical_name: "farm:feed-chickens".to_string(),
                    component_type: ComponentType::Skill,
                    file_path: "/farm/feed-chickens.md".to_string(),
                    source: "plugin:farm".to_string(),
                },
            );

            components.insert(
                "unique-skill".to_string(),
                ResolverComponent {
                    name: "unique-skill".to_string(),
                    namespace: None,
                    canonical_name: "unique-skill".to_string(),
                    component_type: ComponentType::Skill,
                    file_path: "/unique-skill.md".to_string(),
                    source: "user".to_string(),
                },
            );

            Self {
                components,
                aliases,
            }
        }

        fn with_alias(mut self, alias: &str, target: &str) -> Self {
            self.aliases.insert(alias.to_string(), target.to_string());
            self
        }
    }

    impl NamespacedRegistry for MockRegistry {
        fn get_by_canonical(&self, canonical_name: &str) -> Option<ResolverComponent> {
            self.components.get(canonical_name).cloned()
        }

        fn resolve_alias(&self, alias: &str) -> Option<String> {
            self.aliases.get(alias).cloned()
        }

        fn get_all(&self) -> Vec<ResolverComponent> {
            self.components.values().cloned().collect()
        }
    }

    #[test]
    fn test_canonical_resolution() {
        let registry = MockRegistry::new();
        let resolver = NamespacedResolver::new(registry);

        let result = resolver.resolve("farm:call-horse", None, None);

        match result {
            NameResolutionResult::Canonical(r) => {
                assert_eq!(r.requested_name, "farm:call-horse");
                assert_eq!(r.component.canonical_name, "farm:call-horse");
            }
            _ => panic!("Expected Canonical resolution, got {:?}", result),
        }
    }

    #[test]
    fn test_canonical_not_found() {
        let registry = MockRegistry::new();
        let resolver = NamespacedResolver::new(registry);

        let result = resolver.resolve("unknown:skill", None, None);

        assert!(matches!(result, NameResolutionResult::NotFound(_)));
    }

    #[test]
    fn test_alias_resolution() {
        let registry = MockRegistry::new().with_alias("ch", "farm:call-horse");
        let resolver = NamespacedResolver::new(registry);

        let result = resolver.resolve("ch", None, None);

        match result {
            NameResolutionResult::Alias(r) => {
                assert_eq!(r.requested_name, "ch");
                assert_eq!(r.alias, "ch");
                assert_eq!(r.canonical_name, "farm:call-horse");
                assert_eq!(r.component.canonical_name, "farm:call-horse");
            }
            _ => panic!("Expected Alias resolution, got {:?}", result),
        }
    }

    #[test]
    fn test_local_first_resolution() {
        let registry = MockRegistry::new();
        let resolver = NamespacedResolver::new(registry);

        let context = ResolutionContext {
            caller_namespace: Some("farm".to_string()),
        };

        let result = resolver.resolve("call-horse", Some(&context), None);

        match result {
            NameResolutionResult::LocalFirst(r) => {
                assert_eq!(r.requested_name, "call-horse");
                assert_eq!(r.matched_namespace, "farm");
                assert_eq!(r.component.canonical_name, "farm:call-horse");
            }
            _ => panic!("Expected LocalFirst resolution, got {:?}", result),
        }
    }

    #[test]
    fn test_shorthand_resolution_unique() {
        let registry = MockRegistry::new();
        let resolver = NamespacedResolver::new(registry);

        let result = resolver.resolve("feed-chickens", None, None);

        match result {
            NameResolutionResult::Shorthand(r) => {
                assert_eq!(r.requested_name, "feed-chickens");
                assert_eq!(r.component.canonical_name, "farm:feed-chickens");
            }
            _ => panic!("Expected Shorthand resolution, got {:?}", result),
        }
    }

    #[test]
    fn test_shorthand_resolution_flat_component() {
        let registry = MockRegistry::new();
        let resolver = NamespacedResolver::new(registry);

        let result = resolver.resolve("unique-skill", None, None);

        match result {
            NameResolutionResult::Shorthand(r) => {
                assert_eq!(r.requested_name, "unique-skill");
                assert_eq!(r.component.canonical_name, "unique-skill");
                assert!(r.component.namespace.is_none());
            }
            _ => panic!("Expected Shorthand resolution, got {:?}", result),
        }
    }

    #[test]
    fn test_ambiguous_resolution() {
        let registry = MockRegistry::new();
        let resolver = NamespacedResolver::new(registry);

        let result = resolver.resolve("call-horse", None, None);

        match result {
            NameResolutionResult::Ambiguous(r) => {
                assert_eq!(r.requested_name, "call-horse");
                assert_eq!(r.candidates.len(), 2);
                let canonical_names: Vec<_> = r
                    .candidates
                    .iter()
                    .map(|c| c.canonical_name.as_str())
                    .collect();
                assert!(canonical_names.contains(&"farm:call-horse"));
                assert!(canonical_names.contains(&"zoo:call-horse"));
            }
            _ => panic!("Expected Ambiguous resolution, got {:?}", result),
        }
    }

    #[test]
    fn test_not_found_resolution() {
        let registry = MockRegistry::new();
        let resolver = NamespacedResolver::new(registry);

        let result = resolver.resolve("non-existent", None, None);

        assert!(matches!(result, NameResolutionResult::NotFound(_)));
    }

    #[test]
    fn test_type_filter() {
        let registry = MockRegistry::new();
        let resolver = NamespacedResolver::new(registry);

        // Filter by agent type (should not find skills)
        let result = resolver.resolve("call-horse", None, Some(ComponentType::Agent));

        assert!(matches!(result, NameResolutionResult::NotFound(_)));
    }

    #[test]
    fn test_resolution_type() {
        let registry = MockRegistry::new();
        let resolver = NamespacedResolver::new(registry);

        let result = resolver.resolve("farm:call-horse", None, None);
        assert_eq!(result.resolution_type(), ResolutionType::Canonical);
    }

    #[test]
    fn test_requested_name() {
        let registry = MockRegistry::new();
        let resolver = NamespacedResolver::new(registry);

        let result = resolver.resolve("farm:call-horse", None, None);
        assert_eq!(result.requested_name(), "farm:call-horse");
    }
}
