//! One registry key as the editor sees it: both tiers of it, and whatever is
//! staged against each.
//!
//! A row carries the project tier even for keys that have none, because the
//! three reasons a project value can be missing are three different messages
//! to the operator: the key is user-level by design, the tree has no
//! `.loom/work`, or the file simply does not set it and inherits instead.
//! Collapsing them to one "not set" would send operators to the wrong fix.

use anyhow::Result;

use crate::user_config::keys::KeySpec;
use crate::user_config::workspace::{self, Workspace};
use crate::user_config::{ConfigValue, Origin, UserConfig};

use super::Scope;

/// One tier's own reading of a key.
pub(crate) struct ScopeValue {
    /// What the tier resolves the key to, whether or not its file sets it.
    pub(crate) value: ConfigValue,
    /// Whether the tier's own file sets the key, as opposed to inheriting it.
    pub(crate) set: bool,
}

/// What the project tier has to say about a key.
pub(crate) enum ProjectTier {
    /// The registry key has no project tier at all — `update.*`, which is
    /// about this operator's loom rather than about this repository.
    Unbacked,
    /// The tree has no `.loom/work` to hold a project tier.
    NoWorkspace,
    /// The project tier resolves the key.
    Present(ScopeValue),
}

/// An edit staged against one scope, reversible until the operator saves.
pub(crate) enum Pending {
    /// A validated new value, with the exact text the operator typed. Keeping
    /// the text is what makes inline editing predictable; keeping the typed
    /// value is what stops the screen and `loom config -k` drifting into two
    /// validators.
    Set {
        /// The characters the operator entered.
        raw: String,
        /// The same text, already accepted by the registry validator.
        value: ConfigValue,
    },
    /// Remove the key from this scope's file, reverting to what it inherits.
    Clear,
}

/// Which tier the daemon actually reads for a key.
///
/// The same rule `config_api::entries::effective` implements, and it has to
/// stay the same rule: a settings screen that disagrees with the daemon about
/// the value in force is worse than no settings screen.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum InForce {
    /// The project file supplies the key, so it shadows the user tier.
    Project,
    /// The user file sets the key and no project file overrides it.
    User,
    /// Neither file sets it; loom's built-in default is what runs.
    BuiltIn,
}

impl InForce {
    /// Whether `scope` is the tier in force — what the `◆` marker asks.
    pub(crate) fn is(self, scope: Scope) -> bool {
        matches!(
            (self, scope),
            (Self::User, Scope::User) | (Self::Project, Scope::Project)
        )
    }
}

/// What a scope's SOURCE column says about a key.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Source {
    /// This scope's file sets the key itself.
    Set,
    /// The project file omits it, so the user tier is what is in force.
    Inherited,
    /// The user file omits it, so loom's built-in is what is in force.
    Default,
    /// The key has no project tier; only the user scope can set it.
    UserOnly,
    /// There is no project file in this tree to set it in.
    NoWorkspace,
}

impl Source {
    /// The word the SOURCE column and the inspector print.
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Set => "set",
            Self::Inherited => "inherited",
            Self::Default => "default",
            Self::UserOnly => "user-only",
            Self::NoWorkspace => "no workspace",
        }
    }
}

/// One visible registry row: both tiers, and the edits staged against each.
pub(crate) struct ConfigRow {
    /// The registry specification that determines this row's name and type.
    spec: &'static KeySpec,
    /// The user tier as `~/.loom/config.toml` currently resolves it.
    user: ScopeValue,
    /// The project tier, or why there is none.
    project: ProjectTier,
    /// Which tier the daemon reads — derived from the files, never from a
    /// staged edit, because nothing is in force until it is written.
    in_force: InForce,
    /// Staged edits indexed by [`Scope::index`], so an edit on one tab
    /// survives switching to the other and back.
    pending: [Option<Pending>; 2],
}

impl ConfigRow {
    /// Build a row from one snapshot of each tier.
    pub(crate) fn new(
        spec: &'static KeySpec,
        config: &UserConfig,
        workspace: Option<&Workspace>,
    ) -> Result<Self> {
        let (user, project, in_force) = tiers(spec, config, workspace)?;
        Ok(Self {
            spec,
            user,
            project,
            in_force,
            pending: [None, None],
        })
    }

    /// Re-read both tiers after a write, leaving staged edits alone: an edit
    /// whose own write failed must stay staged for the operator to retry.
    pub(crate) fn reload(
        &mut self,
        config: &UserConfig,
        workspace: Option<&Workspace>,
    ) -> Result<()> {
        let (user, project, in_force) = tiers(self.spec, config, workspace)?;
        self.user = user;
        self.project = project;
        self.in_force = in_force;
        Ok(())
    }

    /// The typed registry entry this row represents.
    pub(crate) fn spec(&self) -> &'static KeySpec {
        self.spec
    }

    /// The project tier, or why there is none.
    pub(crate) fn project(&self) -> &ProjectTier {
        &self.project
    }

    /// Which tier the daemon reads for this key.
    pub(crate) fn in_force(&self) -> InForce {
        self.in_force
    }

    /// The edit staged against `scope`, if any.
    pub(crate) fn pending(&self, scope: Scope) -> Option<&Pending> {
        self.pending[scope.index()].as_ref()
    }

    /// Stage `pending` against `scope`, replacing whatever was staged there.
    pub(crate) fn stage(&mut self, scope: Scope, pending: Pending) {
        self.pending[scope.index()] = Some(pending);
    }

    /// Drop the edit staged against `scope`, once its write has landed.
    pub(crate) fn clear_pending(&mut self, scope: Scope) {
        self.pending[scope.index()] = None;
    }

    /// Whether `scope` still needs an explicit save.
    pub(crate) fn is_modified(&self, scope: Scope) -> bool {
        self.pending(scope).is_some()
    }

    /// Whether `scope`'s own file sets this key.
    pub(crate) fn is_set(&self, scope: Scope) -> bool {
        match scope {
            Scope::User => self.user.set,
            Scope::Project => matches!(&self.project, ProjectTier::Present(value) if value.set),
        }
    }

    /// What `scope` resolves this key to on disk, before any staged edit.
    /// `None` when the scope has nothing to resolve.
    ///
    /// Resolution, not authorship: for a project-backed key the project file
    /// does not supply, this is the user tier's value, because that is what
    /// the project tier leaves in force. A caller asking what the FILE says
    /// wants [`Self::set_value`] instead.
    pub(crate) fn disk_value(&self, scope: Scope) -> Option<&ConfigValue> {
        match scope {
            Scope::User => Some(&self.user.value),
            Scope::Project => match &self.project {
                ProjectTier::Present(value) => Some(&value.value),
                ProjectTier::Unbacked | ProjectTier::NoWorkspace => None,
            },
        }
    }

    /// The tier's value only when that tier's own file sets the key, so a
    /// caller that means "what does the OTHER file say" cannot be handed the
    /// value this tier merely falls through to.
    ///
    /// Without this, every project-backed key the project file omits reads
    /// `project <the user's own value>`, which is true of nearly every row and
    /// therefore tells the operator nothing.
    pub(crate) fn set_value(&self, scope: Scope) -> Option<&ConfigValue> {
        self.disk_value(scope).filter(|_| self.is_set(scope))
    }

    /// What `scope` shows, staged edits included. `None` when the scope has
    /// nothing to show — a user-only key's project tier, or a tree with no
    /// `.loom/work`.
    pub(crate) fn displayed(&self, scope: Scope) -> Option<ConfigValue> {
        match self.pending(scope) {
            Some(Pending::Set { value, .. }) => Some(value.clone()),
            Some(Pending::Clear) => Some(self.inherited(scope)),
            None => self.disk_value(scope).cloned(),
        }
    }

    /// The same, as text. A staged edit keeps the exact characters the
    /// operator typed rather than a re-rendering of the parsed value.
    pub(crate) fn displayed_text(&self, scope: Scope) -> Option<String> {
        match self.pending(scope) {
            Some(Pending::Set { raw, .. }) => Some(raw.clone()),
            _ => self.displayed(scope).map(|value| value.to_string()),
        }
    }

    /// What clearing this key at `scope` would leave in force: the user tier
    /// under the project file, loom's built-in under the user file.
    pub(crate) fn inherited(&self, scope: Scope) -> ConfigValue {
        match scope {
            Scope::User => self.built_in(),
            Scope::Project => self.user.value.clone(),
        }
    }

    /// The value loom resolves with neither file setting the key. Read
    /// through the resolved getters against an all-`None` config, which is
    /// where each default is spelled out — never a second table here.
    pub(crate) fn built_in(&self) -> ConfigValue {
        UserConfig::default().value_of(self.spec).0
    }

    /// What the SOURCE column says for `scope`.
    pub(crate) fn source(&self, scope: Scope) -> Source {
        match scope {
            Scope::User => {
                if self.user.set {
                    Source::Set
                } else {
                    Source::Default
                }
            }
            Scope::Project => match &self.project {
                ProjectTier::Unbacked => Source::UserOnly,
                ProjectTier::NoWorkspace => Source::NoWorkspace,
                ProjectTier::Present(value) if value.set => Source::Set,
                ProjectTier::Present(_) => Source::Inherited,
            },
        }
    }
}

/// Resolve both tiers and the one in force for `spec`.
///
/// The in-force rule is asked of [`Workspace::shadows`] rather than re-derived
/// from whether the project file happens to hold the key: `context.ceiling_tokens`
/// is supplied by `model_window_tokens` too, and a second derivation here
/// would get that key wrong exactly where it matters.
fn tiers(
    spec: &'static KeySpec,
    config: &UserConfig,
    workspace: Option<&Workspace>,
) -> Result<(ScopeValue, ProjectTier, InForce)> {
    let (value, origin) = config.value_of(spec);
    let user = ScopeValue {
        value,
        set: origin == Origin::Set,
    };
    let project = if !workspace::backs(spec) {
        ProjectTier::Unbacked
    } else if let Some(workspace) = workspace {
        ProjectTier::Present(ScopeValue {
            value: workspace.value_of(spec)?,
            set: workspace.has_key(spec),
        })
    } else {
        ProjectTier::NoWorkspace
    };
    let in_force = if workspace.is_some_and(|workspace| workspace.shadows(spec)) {
        InForce::Project
    } else if user.set {
        InForce::User
    } else {
        InForce::BuiltIn
    };
    Ok((user, project, in_force))
}
