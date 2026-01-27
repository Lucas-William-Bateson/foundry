import { BrowserRouter, Routes, Route } from "react-router-dom";
import { Layout } from "@/components/Layout";
import { Dashboard } from "@/pages/Dashboard";
import { JobDetailPage } from "@/pages/JobDetail";
import { Repositories } from "@/pages/Repositories";
import { Schedules } from "@/pages/Schedules";
import { AuthProvider, RequireAuth } from "@/lib/auth";

function App() {
  return (
    <AuthProvider>
      <BrowserRouter>
        <RequireAuth>
          <Routes>
            <Route path="/" element={<Layout />}>
              <Route index element={<Dashboard />} />
              <Route path="job/:id" element={<JobDetailPage />} />
              <Route path="repos" element={<Repositories />} />
              <Route path="schedules" element={<Schedules />} />
            </Route>
          </Routes>
        </RequireAuth>
      </BrowserRouter>
    </AuthProvider>
  );
}

export default App;
