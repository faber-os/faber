import { mount } from 'svelte';
import './app.css';
import App from './ui/App.svelte';
import { lab } from './ui/lab.svelte';

mount(App, { target: document.getElementById('app')! });

// For poking at the lab from the browser's console while developing.
if (import.meta.env.DEV) (window as unknown as { lab: typeof lab }).lab = lab;
