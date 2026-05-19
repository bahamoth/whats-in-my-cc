import { Navigate, Route, Routes } from 'react-router-dom';
import SessionListPage from './routes/SessionListPage';
import SessionDetailPage from './routes/SessionDetailPage';

export default function App() {
  return (
    <Routes>
      <Route path="/" element={<Navigate to="/sessions" replace />} />
      <Route path="/sessions" element={<SessionListPage />} />
      <Route path="/sessions/:sessionId" element={<SessionDetailPage />} />
      <Route path="*" element={<div style={{ padding: 24 }}>not found</div>} />
    </Routes>
  );
}
