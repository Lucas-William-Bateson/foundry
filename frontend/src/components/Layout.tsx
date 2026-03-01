import { useState, useCallback } from "react";
import { Outlet, useLocation, useNavigate } from "react-router-dom";
import { useAuth } from "@/lib/auth";
import AppBar from "@mui/material/AppBar";
import Toolbar from "@mui/material/Toolbar";
import Typography from "@mui/material/Typography";
import IconButton from "@mui/material/IconButton";
import Button from "@mui/material/Button";
import Drawer from "@mui/material/Drawer";
import List from "@mui/material/List";
import ListItemButton from "@mui/material/ListItemButton";
import ListItemIcon from "@mui/material/ListItemIcon";
import ListItemText from "@mui/material/ListItemText";
import Box from "@mui/material/Box";
import Tooltip from "@mui/material/Tooltip";
import MenuIcon from "@mui/icons-material/Menu";
import DashboardIcon from "@mui/icons-material/Dashboard";
import AccountTreeIcon from "@mui/icons-material/AccountTree";
import ScheduleIcon from "@mui/icons-material/Schedule";
import LogoutIcon from "@mui/icons-material/Logout";

const navigation = [
  { name: "Dashboard", href: "/", icon: DashboardIcon },
  { name: "Repositories", href: "/repos", icon: AccountTreeIcon },
  { name: "Schedules", href: "/schedules", icon: ScheduleIcon },
];

export function Layout() {
  const location = useLocation();
  const navigate = useNavigate();
  const { email, logout } = useAuth();
  const [drawerOpen, setDrawerOpen] = useState(false);

  const handleNavClick = useCallback(
    (href: string) => {
      navigate(href);
      setDrawerOpen(false);
    },
    [navigate],
  );

  return (
    <Box sx={{ display: "flex", flexDirection: "column", minHeight: "100vh" }}>
      <AppBar position="sticky" elevation={0}>
        <Toolbar sx={{ minHeight: "48px !important", px: { xs: 2, md: 3 } }}>
          <IconButton
            edge="start"
            color="inherit"
            aria-label="Open menu"
            onClick={() => setDrawerOpen(!drawerOpen)}
            sx={{ mr: 2, display: { md: "none" } }}
          >
            <MenuIcon />
          </IconButton>
          <Typography
            variant="h6"
            component="div"
            onClick={() => handleNavClick("/")}
            sx={{
              cursor: "pointer",
              fontWeight: 700,
              fontSize: "0.875rem",
              letterSpacing: "0.02em",
              color: "primary.main",
              mr: 4,
            }}
          >
            FOUNDRY
          </Typography>
          <Box sx={{ display: { xs: "none", md: "flex" }, gap: 0 }}>
            {navigation.map((item) => {
              const isActive =
                location.pathname === item.href ||
                (item.href !== "/" && location.pathname.startsWith(item.href));
              return (
                <Button
                  key={item.name}
                  onClick={() => navigate(item.href)}
                  startIcon={<item.icon sx={{ fontSize: "16px !important" }} />}
                  sx={{
                    color: isActive ? "text.primary" : "text.secondary",
                    borderBottom: isActive
                      ? "1px solid"
                      : "1px solid transparent",
                    borderColor: isActive ? "primary.main" : "transparent",
                    borderRadius: 0,
                    px: 2,
                    py: 1,
                    fontSize: "0.8125rem",
                    textTransform: "none",
                    "&:hover": {
                      bgcolor: "rgba(255, 255, 255, 0.04)",
                      color: "text.primary",
                    },
                  }}
                >
                  {item.name}
                </Button>
              );
            })}
          </Box>
          <Box sx={{ flexGrow: 1 }} />
          {email && (
            <Typography
              variant="body2"
              sx={{ color: "text.secondary", mr: 1, fontSize: "0.75rem" }}
            >
              {email}
            </Typography>
          )}
          <Tooltip title="Sign out">
            <IconButton color="inherit" onClick={logout} size="small">
              <LogoutIcon sx={{ fontSize: 18 }} />
            </IconButton>
          </Tooltip>
        </Toolbar>
      </AppBar>

      {/* Mobile Drawer */}
      <Drawer
        anchor="left"
        open={drawerOpen}
        onClose={() => setDrawerOpen(false)}
        PaperProps={{
          sx: {
            bgcolor: "background.paper",
            width: 240,
            borderRight: "1px solid rgba(255,255,255,0.06)",
          },
        }}
      >
        <List sx={{ pt: 2 }}>
          {navigation.map((item) => {
            const isActive =
              location.pathname === item.href ||
              (item.href !== "/" && location.pathname.startsWith(item.href));
            return (
              <ListItemButton
                key={item.name}
                selected={isActive}
                onClick={() => handleNavClick(item.href)}
                sx={{
                  mx: 1,
                  borderRadius: 1,
                  "&.Mui-selected": {
                    bgcolor: "rgba(198, 93, 0, 0.1)",
                    color: "primary.main",
                    "& .MuiListItemIcon-root": { color: "primary.main" },
                  },
                  "&:hover": {
                    bgcolor: "rgba(255, 255, 255, 0.04)",
                  },
                }}
              >
                <ListItemIcon
                  sx={{
                    color: isActive ? "primary.main" : "text.secondary",
                    minWidth: 36,
                  }}
                >
                  <item.icon sx={{ fontSize: 20 }} />
                </ListItemIcon>
                <ListItemText
                  primary={item.name}
                  primaryTypographyProps={{ fontSize: "0.875rem" }}
                />
              </ListItemButton>
            );
          })}
        </List>
      </Drawer>

      {/* Main Content */}
      <Box
        component="main"
        className="page-enter"
        sx={{
          flexGrow: 1,
          p: { xs: 2, md: 3 },
          maxWidth: "1200px",
          width: "100%",
          mx: "auto",
        }}
      >
        <Outlet />
      </Box>
    </Box>
  );
}
