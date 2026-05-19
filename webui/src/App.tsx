import { Navigate, Route, Routes } from 'react-router-dom';
import SessionListPage from './routes/SessionListPage';

export default function App() {
  return (
    <Routes>
      <Route path="/" element={<Navigate to="/sessions" replace />} />
      <Route path="/sessions" element={<SessionListPage />} />
      <Route path="/sessions/:sessionId" element={<div>session detail (todo)</div>} />
      <Route path="*" element={<div>not found</div>} />
    </Routes>
  );
}
