import {
  createContext,
  useContext,
  useEffect,
  useState,
  useMemo,
  useCallback,
  type ReactNode,
} from "react";

interface AuthState {
  authenticated: boolean;
  email: string | null;
  name: string | null;
  loading: boolean;
}

interface AuthContextType extends AuthState {
  login: () => void;
  logout: () => void;
  checkAuth: () => Promise<void>;
}

const AuthContext = createContext<AuthContextType | null>(null);

async function fetchAuthStatus(): Promise<AuthState> {
  try {
    const response = await fetch("/auth/status", {
      credentials: "include",
    });
    
    if (response.ok) {
      const data = await response.json();
      return {
        authenticated: data.authenticated,
        email: data.email,
        name: data.name,
        loading: false,
      };
    }
  } catch (error) {
    console.error("Auth check failed:", error);
  }
  
  return {
    authenticated: false,
    email: null,
    name: null,
    loading: false,
  };
}

export function AuthProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<AuthState>({
    authenticated: false,
    email: null,
    name: null,
    loading: true,
  });

  const checkAuth = useCallback(async () => {
    const authState = await fetchAuthStatus();
    setState(authState);
  }, []);

  useEffect(() => {
    let mounted = true;
    
    fetchAuthStatus().then((authState) => {
      if (mounted) {
        setState(authState);
      }
    });
    
    return () => {
      mounted = false;
    };
  }, []);

  const login = useCallback(() => {
    globalThis.location.href = "/auth/login";
  }, []);

  const logout = useCallback(() => {
    globalThis.location.href = "/auth/logout";
  }, []);

  const value = useMemo(
    () => ({
      ...state,
      login,
      logout,
      checkAuth,
    }),
    [state, login, logout, checkAuth]
  );

  return (
    <AuthContext.Provider value={value}>
      {children}
    </AuthContext.Provider>
  );
}

// eslint-disable-next-line react-refresh/only-export-components
export function useAuth() {
  const context = useContext(AuthContext);
  if (!context) {
    throw new Error("useAuth must be used within an AuthProvider");
  }
  return context;
}

export function RequireAuth({ children }: { children: ReactNode }) {
  const { authenticated, loading, login } = useAuth();

  if (loading) {
    return (
      <div className="flex items-center justify-center min-h-screen bg-gray-900">
        <div className="text-white">Loading...</div>
      </div>
    );
  }

  if (!authenticated) {
    return <LoginPage onLogin={login} />;
  }

  return <>{children}</>;
}

function LoginPage({ onLogin }: { onLogin: () => void }) {
  return (
    <div className="flex items-center justify-center min-h-screen bg-gray-900">
      <div className="text-center p-8 bg-gray-800 rounded-lg shadow-xl max-w-md w-full mx-4">
        <div className="mb-8">
          <h1 className="text-3xl font-bold text-white mb-2">Foundry</h1>
          <p className="text-gray-400">CI/CD Pipeline Dashboard</p>
        </div>
        
        <p className="text-gray-300 mb-6">
          Sign in to access your deployment dashboard
        </p>
        
        <button
          onClick={onLogin}
          className="w-full px-6 py-3 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-lg transition-colors flex items-center justify-center gap-2"
        >
          <svg
            className="w-5 h-5"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M11 16l-4-4m0 0l4-4m-4 4h14m-5 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h7a3 3 0 013 3v1"
            />
          </svg>
          Sign in with SSO
        </button>
        
        <p className="mt-6 text-sm text-gray-500">
          Authentication powered by Keycloak
        </p>
      </div>
    </div>
  );
}
