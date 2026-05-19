import { JsonView as JsonViewLite, defaultStyles } from 'react-json-view-lite';
import 'react-json-view-lite/dist/index.css';

export function JsonView({ data }: { data: unknown }) {
  return <JsonViewLite data={data as object} style={defaultStyles} shouldExpandNode={(level) => level < 1} />;
}
