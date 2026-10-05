export interface UserProfile {
  id: string;
  name: string;
  email: string;
  phone: string;
  is_verified: boolean;
  national_id_number?: string;
  role: "user" | "admin";
}

export interface AuthState {
  user: UserProfile | null;
  token: string | null;
  isAuthenticated: boolean;
}
