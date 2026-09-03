import { mount } from 'svelte';
import './styles.css';
import Harness from './Harness.svelte';

mount(Harness, { target: document.getElementById('app')! });
