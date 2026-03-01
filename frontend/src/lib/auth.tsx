import {
  createContext,
  useContext,
  useEffect,
  useState,
  useMemo,
  useCallback,
  type ReactNode,
} from "react";
import Box from "@mui/material/Box";
import MuiButton from "@mui/material/Button";
import Typography from "@mui/material/Typography";
import Paper from "@mui/material/Paper";
import CircularProgress from "@mui/material/CircularProgress";
import LoginIcon from "@mui/icons-material/Login";

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
    [state, login, logout, checkAuth],
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
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
      <Box
        sx={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          minHeight: "100vh",
          bgcolor: "background.default",
        }}
      >
        <CircularProgress color="primary" />
      </Box>
    );
  }

  if (!authenticated) {
    return <LoginPage onLogin={login} />;
  }

  return <>{children}</>;
}

function LoginPage({ onLogin }: { onLogin: () => void }) {
  return (
    <Box
      sx={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        minHeight: "100vh",
        bgcolor: "#0B0F14",
      }}
    >
      <Paper
        elevation={0}
        sx={{
          textAlign: "center",
          p: 4,
          maxWidth: 380,
          width: "100%",
          mx: 2,
          bgcolor: "#111827",
          border: "1px solid rgba(255,255,255,0.06)",
        }}
      >
        <Typography
          variant="h5"
          fontWeight={700}
          sx={{
            mb: 0.5,
            letterSpacing: "0.02em",
            color: "primary.main",
            fontSize: "0.875rem",
            textTransform: "uppercase",
          }}
        >
          FOUNDRY
        </Typography>
        <Typography
          variant="body2"
          color="text.secondary"
          sx={{ mb: 4, fontSize: "0.8125rem" }}
        >
          CI/CD Pipeline Dashboard
        </Typography>
        <Typography
          variant="body2"
          color="text.secondary"
          sx={{ mb: 3, fontSize: "0.8125rem" }}
        >
          Sign in to access your deployment dashboard
        </Typography>
        <MuiButton
          fullWidth
          variant="contained"
          size="large"
          startIcon={<LoginIcon />}
          onClick={onLogin}
          sx={{
            py: 1.25,
            fontWeight: 600,
            fontSize: "0.8125rem",
            borderRadius: "6px",
          }}
        >
          Sign in with SSO
        </MuiButton>
        <Typography
          variant="caption"
          color="text.secondary"
          sx={{ mt: 3, display: "block", fontSize: "0.6875rem" }}
        >
          Authentication powered by WorkOS
        </Typography>
      </Paper>
    </Box>
  );
}
