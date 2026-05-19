import { Navigate, Route, Routes } from 'react-router-dom';

export default function App() {
  return (
    <Routes>
      <Route path="/" element={<Navigate to="/sessions" replace />} />
      <Route path="/sessions" element={<div>session list (todo)</div>} />
      <Route path="/sessions/:sessionId" element={<div>session detail (todo)</div>} />
      <Route path="*" element={<div>not found</div>} />
    </Routes>
  );
}
