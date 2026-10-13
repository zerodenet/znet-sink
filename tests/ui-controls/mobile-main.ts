import { mount } from 'svelte';
import './styles.css';
import MobileShellFixture from './MobileShellFixture.svelte';
import MobileOverviewFixture from './MobileOverviewFixture.svelte';
const fixture = new URLSearchParams(window.location.search).get('fixture') === 'overview' ? MobileOverviewFixture : MobileShellFixture;
mount(fixture, { target: document.getElementById('app')! });
