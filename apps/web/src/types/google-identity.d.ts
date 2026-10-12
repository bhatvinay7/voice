interface GoogleIdentityCredentialResponse {
  credential?: string;
}

interface GoogleIdentityButtonOptions {
  theme?: "outline" | "filled_blue" | "filled_black";
  size?: "large" | "medium" | "small";
  shape?: "rectangular" | "pill" | "circle" | "square";
  text?: "signin_with" | "signup_with" | "continue_with" | "signin";
  width?: number;
}

interface GoogleIdentityApi {
  initialize(options: {
    client_id: string;
    callback: (response: GoogleIdentityCredentialResponse) => void;
  }): void;
  renderButton(element: HTMLElement, options: GoogleIdentityButtonOptions): void;
}

declare interface Window {
  google?: {
    accounts?: {
      id?: GoogleIdentityApi;
    };
  };
}
