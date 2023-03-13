// Import our custom CSS
import 'bootstrap/scss/bootstrap.scss';
import 'bootstrap-icons/font/bootstrap-icons.scss'
import '../scss/styles.scss';

import React from 'react';
import ReactDOMClient from 'react-dom/client';
import App from "./App";

const root = ReactDOMClient.createRoot(document.getElementById('root'));
root.render(<App/>)

