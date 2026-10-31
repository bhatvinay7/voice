use crate::models::UserProfile;
use super::{DatabaseStore, DbResult};

impl DatabaseStore {
    pub async fn get_user(&self, user_id: &str) -> DbResult<Option<UserProfile>> { self.one(user_id).await }
    pub async fn get_user_by_email(&self, email: &str) -> DbResult<Option<UserProfile>> {
        Ok(self.fetch("SELECT row_to_json(t)::text FROM public.user_profiles t WHERE lower(email) = lower($1) ORDER BY id LIMIT 1", &[email]).await?.pop())
    }
    pub async fn update_user_role(&self, user_id: &str, new_role: &str) -> DbResult<Option<UserProfile>> {
        Ok(self.fetch("UPDATE public.user_profiles t SET role = $2 WHERE id = $1 RETURNING row_to_json(t)::text", &[user_id, new_role]).await?.pop())
    }
    pub async fn get_all_users(&self) -> DbResult<Vec<UserProfile>> { self.all().await }
    pub async fn verify_user_document(&self, user_id: &str, emirates_id: &str) -> DbResult<UserProfile> {
        self.fetch("UPDATE public.user_profiles t SET is_verified = true, emirates_id_number = $2 WHERE id = $1 RETURNING row_to_json(t)::text", &[user_id, emirates_id]).await?.pop().ok_or_else(|| "User not found".into())
    }
    pub async fn upsert_user(&self, user_id: &str, name: &str, email: &str, default_role: &str, auth_method: &str) -> DbResult<UserProfile> {
        self.fetch("INSERT INTO public.user_profiles AS t (id, name, email, phone, is_verified, role, auth_methods) VALUES ($1, $2, $3, '', false, $4, ARRAY[$5]) ON CONFLICT (id) DO UPDATE SET auth_methods = CASE WHEN $5 = ANY(t.auth_methods) THEN t.auth_methods ELSE array_append(t.auth_methods, $5) END RETURNING row_to_json(t)::text", &[user_id, name, email, default_role, auth_method]).await?.pop().ok_or_else(|| "User not saved".into())
    }
}
