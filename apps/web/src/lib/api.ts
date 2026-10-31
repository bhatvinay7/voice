import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { PropertyListing } from "../../types/listing";
import { UserProfile } from "../../types/user";
import { BookingResponse } from "../../types/booking";
import { TenancyContract } from "../../types/contract";
import { AdminGraphNode } from "../../types/graph";
import { StreamEventRecord, CallSummaryRecord } from "../../types/log";
import { AdminSessionDetailsResponse } from "../../types/admin";

const API_BASE = "/api";

async function fetchJson<T>(url: string, options?: RequestInit): Promise<T> {
  const token = typeof window !== "undefined" ? localStorage.getItem("auth_token") : null;
  const res = await fetch(`${API_BASE}${url}`, {
    headers: {
      "Content-Type": "application/json",
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
      ...options?.headers,
    },
    ...options,
  });
  if (!res.ok) {
    const errorBody = await res.json().catch(() => null) as { error?: string; message?: string } | null;
    throw new Error(errorBody?.error || errorBody?.message || `HTTP ${res.status}: ${res.statusText}`);
  }
  return res.json() as Promise<T>;
}

// Listing Hooks
export function useListings() {
  return useQuery({
    queryKey: ["listings"],
    queryFn: () => fetchJson<{ success: boolean; listings: PropertyListing[] }>("/listings"),
  });
}

// User Profile & Verification Hooks
export function useUserProfile() {
  return useQuery({
    queryKey: ["user", "me"],
    queryFn: () => fetchJson<{ success: boolean; user: UserProfile | null }>("/auth/me"),
  });
}

export function useVerifyDocument() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (params: { userId: string; nationalIdNumber: string }) =>
      fetchJson<{ success: boolean; user: UserProfile }>("/users/verify", {
        method: "POST",
        body: JSON.stringify({
          user_id: params.userId,
          national_id_number: params.nationalIdNumber,
        }),
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["user", "me"] });
    },
  });
}

// Booking Hook
export function useBookViewingSlot() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (params: {
      listingId: string;
      userId: string;
      slotTime: string;
      idempotencyKey: string;
      sessionId?: string;
    }) =>
      fetchJson<BookingResponse>("/bookings", {
        method: "POST",
        body: JSON.stringify({
          listing_id: params.listingId,
          user_id: params.userId,
          slot_time: params.slotTime,
          idempotency_key: params.idempotencyKey,
          ...(params.sessionId ? { session_id: params.sessionId } : {}),
        }),
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["listings"] });
    },
  });
}

// Contract Hooks
export function useContractReviewQueue() {
  return useQuery({
    queryKey: ["contracts", "reviewQueue"],
    queryFn: () => fetchJson<{ success: boolean; review_queue: TenancyContract[] }>("/contracts/review-queue"),
  });
}

export function useApproveContract() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (contractId: string) =>
      fetchJson<{ success: boolean; contract: TenancyContract }>("/contracts/approve", {
        method: "POST",
        body: JSON.stringify({ contract_id: contractId }),
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["contracts", "reviewQueue"] });
    },
  });
}

// Admin Hooks
export function useAdminGraphNodes() {
  return useQuery({
    queryKey: ["admin", "graphNodes"],
    queryFn: () => fetchJson<{ success: boolean; graph_nodes: AdminGraphNode[] }>("/admin/graph"),
  });
}

export function useAdminAddGraphNode() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (node: AdminGraphNode) =>
      fetchJson<{ success: boolean; node: AdminGraphNode }>("/admin/graph/nodes", {
        method: "POST",
        body: JSON.stringify(node),
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["admin", "graphNodes"] });
    },
  });
}

export function useAdminUpdateGraphNode() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (params: { id: string; instruction: string }) =>
      fetchJson<{ success: boolean }>(`/admin/graph/nodes/${params.id}`, {
        method: "PUT",
        body: JSON.stringify({ instruction: params.instruction }),
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["admin", "graphNodes"] });
    },
  });
}

export function useAdminDeleteGraphNode() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) =>
      fetchJson<{ success: boolean }>(`/admin/graph/nodes/${id}`, {
        method: "DELETE",
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["admin", "graphNodes"] });
    },
  });
}

export function useAdminLogs() {
  return useQuery({
    queryKey: ["admin", "logs"],
    queryFn: () => fetchJson<{ success: boolean; logs: StreamEventRecord[] }>("/admin/logs"),
  });
}

export function useAdminCallSummaries() {
  return useQuery({
    queryKey: ["admin", "callSummaries"],
    queryFn: () => fetchJson<{ success: boolean; summaries: CallSummaryRecord[] }>("/admin/call-summaries"),
  });
}

export function useAdminSessionDetails(sessionId: string) {
  return useQuery({
    queryKey: ["admin", "session", sessionId],
    queryFn: () => fetchJson<AdminSessionDetailsResponse>(`/admin/sessions/${sessionId}`),
    enabled: !!sessionId,
  });
}

export function useAdminAddListing() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (listing: PropertyListing) =>
      fetchJson<{ success: boolean; listing: PropertyListing }>("/admin/listings", {
        method: "POST",
        body: JSON.stringify(listing),
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["listings"] });
    },
  });
}

export function useAdminUpdateListing() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (params: { id: string; listing: PropertyListing }) =>
      fetchJson<{ success: boolean; listing: PropertyListing }>(`/admin/listings/${params.id}`, {
        method: "PUT",
        body: JSON.stringify(params.listing),
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["listings"] });
    },
  });
}

export function useAdminDeleteListing() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) =>
      fetchJson<{ success: boolean }>(`/admin/listings/${id}`, {
        method: "DELETE",
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["listings"] });
    },
  });
}

export function useAdminLogin() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (params: { email: string; password: string }) =>
      fetchJson<{ success: boolean; token: string; user: UserProfile }>("/admin/login", {
        method: "POST",
        body: JSON.stringify(params),
      }),
    onSuccess: (data) => {
      if (typeof window !== "undefined" && data.token) {
        localStorage.setItem("auth_token", data.token);
      }
      queryClient.setQueryData(["user", "me"], { success: true, user: data.user });
      queryClient.invalidateQueries({ queryKey: ["user", "me"] });
    },
  });
}

export function useEmailLogin() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (params: { email: string; password: string; name?: string }) =>
      fetchJson<{ success: boolean; token: string; user: UserProfile }>("/auth/login", {
        method: "POST",
        body: JSON.stringify(params),
      }),
    onSuccess: (data) => {
      if (typeof window !== "undefined" && data.token) {
        localStorage.setItem("auth_token", data.token);
      }
      queryClient.setQueryData(["user", "me"], { success: true, user: data.user });
      queryClient.invalidateQueries({ queryKey: ["user", "me"] });
    },
  });
}

export function useGoogleLogin() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (params: { idToken: string }) =>
      fetchJson<{ success: boolean; token: string; user: UserProfile }>("/auth/google", {
        method: "POST",
        body: JSON.stringify({ id_token: params.idToken }),
      }),
    onSuccess: (data) => {
      if (typeof window !== "undefined" && data.token) {
        localStorage.setItem("auth_token", data.token);
      }
      queryClient.setQueryData(["user", "me"], { success: true, user: data.user });
      queryClient.invalidateQueries({ queryKey: ["user", "me"] });
    },
  });
}

export function useLogout() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () =>
      fetchJson<{ success: boolean; message: string }>("/auth/logout", {
        method: "POST",
      }),
    onSuccess: () => {
      if (typeof window !== "undefined") {
        localStorage.removeItem("auth_token");
      }
      queryClient.setQueryData(["user", "me"], { success: true, user: null });
      queryClient.invalidateQueries({ queryKey: ["user", "me"] });
    },
  });
}
