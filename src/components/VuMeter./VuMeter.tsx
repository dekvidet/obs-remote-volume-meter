import VuChannel from '../VuChannel/VuChannel'
import './VuMeter.css'

type VuMeterProps = {
  volume: number
}

function VuMeter({ volume }: VuMeterProps) {

  const percent = volume

  return (
    <div className="vuMeter">
      <VuChannel volume={percent}/>
      <VuChannel volume={percent}/>
    </div>
  )
}

export default VuMeter
