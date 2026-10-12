use crate::models::UserProfile;
use super::{this_lock, DatabaseStore};

impl DatabaseStore {
    pub fn get_user(&self, user_id: &str) -> Option<UserProfile> {
        let guard = match this_lock(&self.users) {
            Ok(g) => g,
            Err(_) => return None,
        };
        guard.get(user_id).cloned()
    }

    pub fn get_user_by_email(&self, email: &str) -> Option<UserProfile> {
        let guard = match this_lock(&self.users) {
            Ok(g) => g,
            Err(_) => return None,
        };
        guard.values().find(|u| u.email.eq_ignore_ascii_case(email)).cloned()
    }

    pub fn update_user_role(&self, user_id: &str, new_role: &str) -> Option<UserProfile> {
        let mut guard = match this_lock(&self.users) {
            Ok(g) => g,
            Err(_) => return None,
        };
        if let Some(user) = guard.get_mut(user_id) {
            user.role = new_role.to_string();
            return Some(user.clone());
        }
        None
    }

    pub fn get_all_users(&self) -> Vec<UserProfile> {
        let guard = match this_lock(&self.users) {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        guard.values().cloned().collect()
    }

    pub fn verify_user_document(&self, user_id: &str, emirates_id: &str) -> UserProfile {
        let mut guard = match this_lock(&self.users) {
            Ok(g) => g,
            Err(_) => {
                return UserProfile {
                    id: user_id.to_string(),
                    name: "Guest".to_string(),
                    email: "guest@example.com".to_string(),
                    phone: "+971500000000".to_string(),
                    is_verified: true,
                    emirates_id_number: Some(emirates_id.to_string()),
                    role: "user".to_string(),
                    auth_methods: vec!["email".to_string()],
                }
            }
        };

        let entry = guard
            .entry(user_id.to_string())
            .or_insert_with(|| UserProfile {
                id: user_id.to_string(),
                name: "Verified User".to_string(),
                email: format!("{}@example.com", user_id),
                phone: "+971500000000".to_string(),
                is_verified: false,
                emirates_id_number: None,
                role: "user".to_string(),
                auth_methods: vec!["email".to_string()],
            });

        entry.is_verified = true;
        entry.emirates_id_number = Some(emirates_id.to_string());
        entry.clone()
    }

    pub fn upsert_user(
        &self,
        user_id: &str,
        name: &str,
        email: &str,
        default_role: &str,
        auth_method: &str,
    ) -> UserProfile {
        let mut guard = match this_lock(&self.users) {
            Ok(g) => g,
            Err(_) => return UserProfile {
                id: user_id.to_string(),
                name: name.to_string(),
                email: email.to_string(),
                phone: "+971500000000".to_string(),
                is_verified: false,
                emirates_id_number: None,
                role: default_role.to_string(),
                auth_methods: vec![auth_method.to_string()],
            },
        };

        let entry = guard.entry(user_id.to_string()).or_insert_with(|| UserProfile {
            id: user_id.to_string(),
            name: name.to_string(),
            email: email.to_string(),
            phone: "+971500000000".to_string(),
            is_verified: false,
            emirates_id_number: None,
            role: default_role.to_string(),
            auth_methods: vec![],
        });
        
        if !entry.auth_methods.contains(&auth_method.to_string()) {
            entry.auth_methods.push(auth_method.to_string());
        }

        entry.clone()
    }
}
