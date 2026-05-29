import { Link } from 'react-router-dom';
import styles from './TopBar.module.css';

interface TopBarProps {
  sessionId: string;
}

export function TopBar({ sessionId }: TopBarProps) {
  return (
    <nav className={styles.bar} aria-label="Breadcrumb">
      <ol className={styles.crumbs}>
        <li>
          <Link to="/sessions" className={styles.link}>
            Sessions
          </Link>
        </li>
        <li aria-hidden="true" className={styles.sep}>
          /
        </li>
        <li>
          <code className={styles.current} aria-current="page">
            {sessionId}
          </code>
        </li>
      </ol>
    </nav>
  );
}
