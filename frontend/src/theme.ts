import { createTheme } from "@mui/material/styles";

// Steel & Ember — Infrastructure Software Palette
const palette = {
  black: "#0B0F14",
  surface: "#111827",
  surfaceRaised: "#151B23",
  border: "rgba(255, 255, 255, 0.06)",
  borderHover: "rgba(255, 255, 255, 0.1)",
  text: "#E8EAED",
  textMuted: "#8B8F96",
  orange: "#C65D00",
  orangeHover: "#D4700F",
  green: "#2D9D5E",
  red: "#D44B4B",
  amber: "#C89520",
  blue: "#5B8DEF",
};

const theme = createTheme({
  palette: {
    mode: "dark",
    primary: { main: palette.orange },
    background: {
      default: palette.black,
      paper: palette.surface,
    },
    text: {
      primary: palette.text,
      secondary: palette.textMuted,
    },
    error: { main: palette.red },
    success: { main: palette.green },
    warning: { main: palette.amber },
    info: { main: palette.blue },
    divider: palette.border,
  },
  typography: {
    fontFamily: "'Inter', -apple-system, BlinkMacSystemFont, sans-serif",
    h1: {
      fontSize: "1.75rem",
      fontWeight: 700,
      letterSpacing: "-0.02em",
      lineHeight: 1.2,
    },
    h2: {
      fontSize: "1.25rem",
      fontWeight: 600,
      letterSpacing: "-0.01em",
      lineHeight: 1.3,
    },
    h3: { fontSize: "1rem", fontWeight: 600, lineHeight: 1.4 },
    body2: { fontSize: "0.8125rem", color: palette.textMuted },
    caption: {
      fontSize: "0.6875rem",
      fontWeight: 500,
      letterSpacing: "0.04em",
      textTransform: "uppercase" as const,
      color: palette.textMuted,
    },
  },
  shape: { borderRadius: 6 },
  components: {
    MuiCssBaseline: {
      styleOverrides: {
        body: { backgroundColor: palette.black },
      },
    },
    MuiAppBar: {
      styleOverrides: {
        root: {
          backgroundColor: palette.black,
          backgroundImage: "none",
          borderBottom: `1px solid ${palette.border}`,
          backdropFilter: "blur(12px)",
        },
      },
      defaultProps: { elevation: 0 },
    },
    MuiPaper: {
      styleOverrides: {
        root: {
          backgroundImage: "none",
          border: `1px solid ${palette.border}`,
          borderRadius: 6,
        },
      },
      defaultProps: { elevation: 0 },
    },
    MuiButton: {
      styleOverrides: {
        root: {
          borderRadius: 6,
          textTransform: "none" as const,
          fontWeight: 500,
          fontSize: "0.8125rem",
        },
      },
    },
    MuiChip: {
      styleOverrides: {
        root: { borderRadius: 4, fontWeight: 500 },
      },
    },
    MuiCard: {
      styleOverrides: {
        root: {
          backgroundColor: palette.surface,
          border: `1px solid ${palette.border}`,
          borderRadius: 6,
          backgroundImage: "none",
        },
      },
      defaultProps: { elevation: 0 },
    },
    MuiTooltip: {
      styleOverrides: {
        tooltip: {
          backgroundColor: palette.surfaceRaised,
          border: `1px solid ${palette.border}`,
          fontSize: "0.75rem",
        },
      },
    },
    MuiListItemButton: {
      styleOverrides: {
        root: { borderRadius: 4 },
      },
    },
  },
});

export { palette };
export default theme;
