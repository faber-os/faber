import '@fontsource-variable/atkinson-hyperlegible-next';
import '@fontsource-variable/atkinson-hyperlegible-mono';
import './styles/tokens.css';
import './styles/base.css';
import './styles/wallpapers.css';
import { mount } from 'svelte';
import App from './App.svelte';

const target = document.getElementById('app');
if (!target) throw new Error('index.html is missing <div id="app">');

export default mount(App, { target });
