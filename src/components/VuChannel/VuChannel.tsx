import './VuChannel.css'

type VuChannelProps = {
  volume: number
}

function VuChannel({ volume }: VuChannelProps) {

  const percent = volume

  return (
    <>
      <div className="vuChannel">
        <div className="box background"></div>
        <div className="box value" style={{clipPath: `rect(0 ${percent}% 100% 0 round 0%)`}}></div>
      </div>
    </>
  )
}

export default VuChannel
