import OBSWebSocket, { EventSubscription } from 'obs-websocket-js'
const obs = new OBSWebSocket();

const OBS_HOST = 'ws://localhost:4455';
const OBS_PASSWORD = 'a3WyXmMpDutl9Seq';

async function connectOBS() {
    try {

        const onInputVolumeMeters = (event) => {
            console.log(event.inputs[0].inputLevelsMul)
        }
        await obs.connect(OBS_HOST, OBS_PASSWORD, {
          eventSubscriptions: EventSubscription.InputVolumeMeters,
          rpcVersion: 1
        });
        obs.on('InputVolumeMeters', onInputVolumeMeters);
        console.log('Connected to OBS WebSocket');
        /*
        
        // Fetch active audio sources and their volume levels
        setInterval(async () => {
            try {
                const { inputs } = await obs.call('GetInputList', { inputKind: 'wasapi_output_capture' });
                console.log(inputs)
                for (const input of inputs) {
                    const volumeInfo = await obs.call('GetInputVolume', { inputName: input.inputName });
                    console.log(`Source: ${input.inputName}, Volume: ${volumeInfo.inputVolumeMul}`);
                }
            } catch (error) {
                console.error('Error fetching audio levels:', error);
            }
        }, 1000); // Poll every second
        
        */
    } catch (error) {
        console.error('Failed to connect to OBS WebSocket:', error);
    }
}

connectOBS();
