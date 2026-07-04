import { Suspense, lazy } from 'react';
import { Navigate, Route, Routes } from 'react-router-dom';
import { AppShell } from './components/layout/AppShell';
import SessionListPage from './routes/SessionListPage';
import SessionDetailPage from './routes/SessionDetailPage';
// 대시보드는 ECharts를 끌고 오므로 lazy 분할 — 리플레이 first-load를 지킨다.
const DashboardPage = lazy(() => import('./routes/DashboardPage'));

export default function App() {
  return (
    <AppShell>
      <Routes>
        <Route path="/" element={<Navigate to="/sessions" replace />} />
        <Route path="/sessions" element={<SessionListPage />} />
        <Route path="/sessions/:sessionId" element={<SessionDetailPage />} />
        <Route
          path="/dashboard"
          element={
            <Suspense fallback={null}>
              <DashboardPage />
            </Suspense>
          }
        />
        <Route path="*" element={<div style={{ padding: 24 }}>not found</div>} />
      </Routes>
    </AppShell>
  );
}
