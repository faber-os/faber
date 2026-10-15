import '@fontsource-variable/hanken-grotesk';
import '@fontsource-variable/red-hat-mono';
import './styles/tokens.css';
import './styles/base.css';
import { mount } from 'svelte';
import App from './App.svelte';

const target = document.getElementById('app');
if (!target) throw new Error('index.html is missing <div id="app">');

export default mount(App, { target });
