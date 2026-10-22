import { mount } from 'svelte';
import './styles.css';
import MobileShellFixture from './MobileShellFixture.svelte';
import MobileOverviewFixture from './MobileOverviewFixture.svelte';
import MobileLocationFixture from './MobileLocationFixture.svelte';
const name = new URLSearchParams(window.location.search).get('fixture');
const fixture = name === 'mobile-location' ? MobileLocationFixture : name === 'overview' ? MobileOverviewFixture : MobileShellFixture;
mount(fixture, { target: document.getElementById('app')! });
