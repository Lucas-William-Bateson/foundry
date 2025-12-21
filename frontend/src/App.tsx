import { BrowserRouter, Routes, Route } from 'react-router-dom'
import { Layout } from '@/components/Layout'
import { Dashboard } from '@/pages/Dashboard'
import { JobDetailPage } from '@/pages/JobDetail'
import { Repositories } from '@/pages/Repositories'

function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<Layout />}>
          <Route index element={<Dashboard />} />
          <Route path="job/:id" element={<JobDetailPage />} />
          <Route path="repos" element={<Repositories />} />
        </Route>
      </Routes>
    </BrowserRouter>
  )
}

export default App
