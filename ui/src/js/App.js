import {Container} from "react-bootstrap";
import React, {useEffect, useState} from "react";


const data_host = process.env.API_HOST;

const noServerAlert = (e) => {
    alert(`no server 🤷 😿 👉 ${JSON.stringify(e)}`);
};

const switchValue = (pin_state) => {
    return pin_state >= 1;
};

const SwitchRow = ({switch_state, onUpdatePinState, onUpdatePinOverride}) => {
    const checked = switchValue(switch_state.pin_state) ? "checked" : "";
    const iconClz = switch_state.is_auto && !switch_state.override_auto ? "bi bi-lock" : "bi bi-unlock";
    const iconBtnClz = switch_state.is_auto && !switch_state.override_auto ? "btn btn-sm btn-primary btn-danger" : "btn btn-sm btn-primary";
    const setPinState = (event) => {
        const sd = event.target.checked;
        if (sd) {
            switch_state.pin_state = 1;
        } else {
            switch_state.pin_state = 0;
        }
        onUpdatePinState(switch_state);
    };

    const setAutoOverride = (event) => {
        console.log(`event: ${event.target}`);
        switch_state.override_auto = !switch_state.override_auto;
        onUpdatePinOverride(switch_state);
    };
    let overrideBtn;
    if (switch_state.is_auto) {
        overrideBtn = (
            <button className={iconBtnClz} onClick={setAutoOverride}>
                <i className={iconClz} id="icon"></i>
            </button>
        )
    }
    return (
        <div className="row border" key={switch_state.pin_num} id={`switch_${switch_state.pin_num}`}>
            <div className="col-2 d-flex align-items-center">
                {overrideBtn}
            </div>
            <div className="col-8 d-flex align-items-center">
                <div className="form-check form-switch text-success form-control-lg">
                    <label className="form-check-label" htmlFor="switch_input"
                           id="col_name">{switch_state.name}</label>
                    <input className="form-check-input" type="checkbox" id={`switch_input_${switch_state.pin_num}`}
                           checked={checked}
                           disabled={switch_state.is_auto && !switch_state.override_auto}
                           onChange={setPinState}/>
                </div>
            </div>
            {/*<div class="col-5 d-flex align-items-center" id="col_name">name</div>*/}
            <div className="col-2 d-flex align-items-center">{switch_state.pin_num}</div>
            {/*<div className="col"></div>*/}
        </div>
    );
}

const SwitchesGrid = ({switches, onUpdatePinState, onUpdatePinOverride}) => {
    return (
        <div className="container border" id="switches_container">
            {switches.map((switch_state) => {
                return (<SwitchRow key={switch_state.pin_num} switch_state={switch_state}
                                   onUpdatePinState={onUpdatePinState} onUpdatePinOverride={onUpdatePinOverride}/>);
            })}
        </div>
    );
};

const Sensor = ({metric}) => {
    return (<li className="list-group-item d-flex align-items-center">
        <div className="container-sm">
            <div className="row">
                <div className="col d-flex align-items-center">{metric.name}</div>
                <div className="col d-flex align-items-center">{metric.value}</div>
            </div>
        </div>
        <br/>
    </li>);
};

const Sensors = ({metrics}) => {
    const metric_lasts = [/*'c', */'f', 'h']

    const metrics_list = metrics.split("\n")
        .map((line) => line.trim())
        .filter((line) => {
            return !line.startsWith("#");
        })
        .map((line) => {
            const parts = line.split(" ");
            return {name: parts[0], value: Number.parseFloat(parts[1]).toFixed(2)};
        })
        .filter((metrics) => {
            const last = metrics.name.substring(metrics.name.length - 1, metrics.name.length);
            return metric_lasts.includes(last);
        });

    return (
        <div>
            <h2>Sensors</h2>
            <ul className="list-group">
                {metrics_list.map((m, i) => {
                    return (<Sensor key={i} metric={m}></Sensor>);
                })}
            </ul>
        </div>
    );
};

const App = () => {
    const [switches, setSwitches] = useState([]);
    const [metrics, setMetrics] = useState('');

    const apiUpdatePinState = (switch_state) => {
        fetch(`${data_host}/pin/output/${switch_state.pin_num}/${switch_state.pin_state}`)
            .then((data) => {
                data.text().then((txt) => {
                    console.log(`status: ${data.status} ${data.statusText}`);
                    console.log(`payload: ${txt}`);
                    if (data.status > 299) {
                        alert(`pin update failed 🤷 😿 👉 ${txt}`);
                    }
                    fetchSwitchesState();
                });

            })
            .catch((err) => {
                noServerAlert(err);
            });
    };

    const apiUpdatePinOverride = (switch_state) => {
        const value = switch_state.override_auto ? 1 : 0;
        fetch(`${data_host}/pin/override_auto/${switch_state.pin_num}/${value}`)
            .then((data) => {
                data.text().then((txt) => {
                    console.log(`status: ${data.status} ${data.statusText}`);
                    console.log(`payload: ${txt}`);
                    if (data.status > 299) {
                        alert(`pin auto override failed 🤷 😿 👉 ${txt}`);
                    }
                    fetchSwitchesState();
                });

            })
            .catch((err) => {
                noServerAlert(err);
            });
    };

    const fetchSwitchesState = () => {
        fetch(`${data_host}/switches_state`)
            .then((response) => response.json())
            .then((data) => {
                console.log(data);
                setSwitches(data.switches);
            })
            .catch((err) => {
                // console.log(err.message);
                noServerAlert(err);
            });
    };

    const fetchMetrics = () => {
        fetch(`${data_host}/metrics`)
            .then((response) => response.text())
            .then((data) => {
                // console.log(data);
                setMetrics(data);
            })
            .catch((err) => {
                // console.log(err.message);
                noServerAlert(err);
            });
    };

    useEffect(() => {
        fetchSwitchesState();
    }, []);

    useEffect(() => {
        fetchMetrics();
    }, []);

    const updatePinState = async (switch_state) => {
        console.log(`updated_switch_state: ${switch_state}`);
        apiUpdatePinState(switch_state);
    };

    const updatePinOverride = async (switch_state) => {
        console.log(`updated_switch_override: ${switch_state}`);
        apiUpdatePinOverride(switch_state);
    };


    return (
        <Container className="p-1">
            <h1>Greenhouse Agent</h1>
            <SwitchesGrid switches={switches} onUpdatePinState={updatePinState}
                          onUpdatePinOverride={updatePinOverride}></SwitchesGrid>
            <Sensors metrics={metrics}></Sensors>
        </Container>
    )
};

export default App;