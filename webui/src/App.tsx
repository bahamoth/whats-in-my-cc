import { Navigate, Route, Routes } from 'react-router-dom';
import { AppShell } from './components/layout/AppShell';
import SessionListPage from './routes/SessionListPage';
import SessionDetailPage from './routes/SessionDetailPage';
import DashboardPage from './routes/DashboardPage';

export default function App() {
  return (
    <AppShell>
      <Routes>
        <Route path="/" element={<Navigate to="/sessions" replace />} />
        <Route path="/sessions" element={<SessionListPage />} />
        <Route path="/sessions/:sessionId" element={<SessionDetailPage />} />
        <Route path="/dashboard" element={<DashboardPage />} />
        <Route path="*" element={<div style={{ padding: 24 }}>not found</div>} />
      </Routes>
    </AppShell>
  );
}
