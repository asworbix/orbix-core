// ─────────────────────────────────────────────
//  Onboarding - first-run state
//
//  Kept deliberately tiny: whether the welcome flow ran and what the
//  user wants to be called (used in Bob's system prompt). The old
//  role-profile system that used to live alongside this was removed -
//  skills, memory, and workspaces are how Orbix adapts to a user now.
//  Stored onboarding.json files from that era carry a selected_role
//  field; serde ignores it.
// ─────────────────────────────────────────────

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct UserOnboarding {
    #[serde(default)]
    pub completed: bool,
    #[serde(default)]
    pub user_name: String,
}
