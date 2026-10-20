"""Fixed Debug Keychain facts; neither a credential export nor production admission."""
STEPS = ('availabilityProbe', 'readBeforeCreate', 'random', 'create', 'readAfterCreate', 'cleanup')
STAGES = ('scope', 'protectedDataGate', 'readBeforeCreate', 'random', 'create', 'readAfterCreate', 'cleanup', 'completed')


def validate_keychain_diagnostics(value):
    assert isinstance(value, dict)
    assert set(value) == {'schemaVersion', 'isolated', 'protectedDataAvailable', 'lastHostAdmission',
                          'readBackMatches', 'stopStage', 'roundTrip', *STEPS}
    assert type(value['schemaVersion']) is int and value['schemaVersion'] == 1
    assert value['isolated'] is True
    assert type(value['protectedDataAvailable']) is bool and type(value['roundTrip']) is bool
    assert value['stopStage'] in STAGES
    assert value['roundTrip'] is (value['stopStage'] == 'completed')

    def step(item):
        assert isinstance(item, dict) and set(item) == {'attempted', 'osStatus'}
        assert type(item['attempted']) is bool
        status = item['osStatus']
        if item['attempted']:
            assert type(status) is int and -(2**31) <= status < 2**31
        else:
            assert status is None

    for key in STEPS:
        step(value[key])
    admission = value['lastHostAdmission']
    if admission is not None:
        assert isinstance(admission, dict) and set(admission) == {'protectedDataAvailable', 'probe', 'apiAvailable'}
        assert type(admission['protectedDataAvailable']) is bool and type(admission['apiAvailable']) is bool
        step(admission['probe'])
        assert admission['protectedDataAvailable'] or not admission['probe']['attempted']
        available = (admission['protectedDataAvailable'] and admission['probe']['attempted']
                     and admission['probe']['osStatus'] in (0, -25300))
        assert admission['apiAvailable'] is available
    assert value['readBackMatches'] is None or type(value['readBackMatches']) is bool
    assert (value['readBackMatches'] is not None) is (value['readAfterCreate']['osStatus'] == 0)
    if value['stopStage'] == 'protectedDataGate':
        assert not value['protectedDataAvailable'] and not any(value[key]['attempted'] for key in STEPS)
    if value['roundTrip']:
        assert value['availabilityProbe']['attempted'] and value['readBackMatches'] is True
        assert value['readBeforeCreate']['osStatus'] == -25300
        assert all(value[key]['osStatus'] == 0 for key in ('random', 'create', 'readAfterCreate'))
        assert value['cleanup']['osStatus'] in (0, -25300)
    return value
