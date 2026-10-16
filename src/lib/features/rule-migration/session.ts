import {RuleMigrationSession} from './state';
import {ruleMigrationPorts} from '$lib/features/rule-migration/client';
/** Original host operation survives the lifetime of its rendering component. */
export const ruleMigrationSession=new RuleMigrationSession(ruleMigrationPorts);
