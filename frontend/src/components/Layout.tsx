import { Link, Outlet, useLocation } from "react-router-dom";
import { cn } from "@/lib/utils";
import {
  Factory,
  LayoutDashboard,
  GitBranch,
  Settings,
  Calendar,
} from "lucide-react";

const navigation = [
  { name: "Dashboard", href: "/", icon: LayoutDashboard },
  { name: "Repositories", href: "/repos", icon: GitBranch },
  { name: "Schedules", href: "/schedules", icon: Calendar },
];

export function Layout() {
  const location = useLocation();

  return (
    <div className="min-h-screen bg-background">
      {/* Sidebar */}
      <div className="fixed inset-y-0 z-50 flex w-64 flex-col">
        <div className="flex grow flex-col gap-y-5 overflow-y-auto border-r bg-card px-6 pb-4">
          <div className="flex h-16 shrink-0 items-center gap-2">
            <Factory className="h-6 w-6 text-primary" />
            <span className="text-xl font-bold">Foundry CI</span>
          </div>
          <nav className="flex flex-1 flex-col">
            <ul role="list" className="flex flex-1 flex-col gap-y-7">
              <li>
                <ul role="list" className="-mx-2 space-y-1">
                  {navigation.map((item) => {
                    const isActive =
                      location.pathname === item.href ||
                      (item.href !== "/" &&
                        location.pathname.startsWith(item.href));
                    return (
                      <li key={item.name}>
                        <Link
                          to={item.href}
                          className={cn(
                            isActive
                              ? "bg-accent text-accent-foreground"
                              : "text-muted-foreground hover:bg-accent hover:text-accent-foreground",
                            "group flex gap-x-3 rounded-md p-2 text-sm font-semibold leading-6",
                          )}
                        >
                          <item.icon
                            className="h-5 w-5 shrink-0"
                            aria-hidden="true"
                          />
                          {item.name}
                        </Link>
                      </li>
                    );
                  })}
                </ul>
              </li>
              <li className="mt-auto">
                <Link
                  to="/settings"
                  className="group -mx-2 flex gap-x-3 rounded-md p-2 text-sm font-semibold leading-6 text-muted-foreground hover:bg-accent hover:text-accent-foreground"
                >
                  <Settings className="h-5 w-5 shrink-0" aria-hidden="true" />
                  Settings
                </Link>
              </li>
            </ul>
          </nav>
        </div>
      </div>

      {/* Main content */}
      <main className="pl-64">
        <div className="px-8 py-6">
          <Outlet />
        </div>
      </main>
    </div>
  );
}
