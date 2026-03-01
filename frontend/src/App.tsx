import { BrowserRouter, Routes, Route } from "react-router-dom";
import { Layout } from "@/components/Layout";
import { Dashboard } from "@/pages/Dashboard";
import { JobDetailPage } from "@/pages/JobDetail";
import { Repositories } from "@/pages/Repositories";
import { RepoDetailPage } from "@/pages/RepoDetail";
import { Schedules } from "@/pages/Schedules";
import { AuthProvider, RequireAuth } from "@/lib/auth";
import { ErrorBoundary } from "@/components/ErrorBoundary";

function App() {
  return (
    <AuthProvider>
      <BrowserRouter>
        <RequireAuth>
          <ErrorBoundary section="Page">
            <Routes>
              <Route path="/" element={<Layout />}>
                <Route index element={<Dashboard />} />
                <Route path="job/:id" element={<JobDetailPage />} />
                <Route path="repos" element={<Repositories />} />
                <Route path="repo/:id" element={<RepoDetailPage />} />
                <Route path="schedules" element={<Schedules />} />
              </Route>
            </Routes>
          </ErrorBoundary>
        </RequireAuth>
      </BrowserRouter>
    </AuthProvider>
  );
}

export default App;
